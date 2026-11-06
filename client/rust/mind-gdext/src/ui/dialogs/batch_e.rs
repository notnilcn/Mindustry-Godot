// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/editor/MapInfoDialog.java,
//         core/src/mindustry/editor/MapResizeDialog.java,
//         core/src/mindustry/editor/MapGenerateDialog.java,
//         core/src/mindustry/editor/WaveInfoDialog.java,
//         core/src/mindustry/editor/MapObjectivesDialog.java,
//         core/src/mindustry/editor/MapLocalesDialog.java
//         (plan 19 M4/M5 §3.9-§3.10).

//! Dialog controller batch E: the map-editor dialogs.
//!
//! Each controller ports one `client/scenes/editor/*.gd` script. All editor
//! state is read and written through [`kit::editor`] with the same dynamic
//! method names as the GDScript (`tags`/`set_tag`, `status`/`resize_map`,
//! `filters_json`/`set_filters_json`/`filter_options_json`, `waves_json`/
//! `set_waves_json`, `objectives_json`/`set_objectives_json`,
//! `locales_json`/`set_locales_json`, …); no controller invents editor state.
//!
//! Because a [`DialogController`] hook gets `&mut self` but widget callbacks
//! are `Callable::from_fn` closures that cannot borrow the controller, the
//! working copies (filters, wave groups, objectives, locale bundles) live in
//! node metadata on the dialog and every callback re-enters through
//! [`MindDialog::refresh_controller`] with a request recorded in
//! [`REQUEST_META`]. `shown` dispatches on that request, mirroring the
//! GDScript `_build`/`_rebuild`/`_build_main` entry points.

use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_edit::LineWrappingMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Button, CenterContainer, ColorPickerButton, ColorRect, Control, DisplayServer, GridContainer,
    HBoxContainer, InputEvent, InputEventMouseButton, Json, Label, LineEdit, MarginContainer, Node,
    PanelContainer, RichTextLabel, StyleBox, StyleBoxFlat, StyleBoxTexture, TextEdit, Texture2D,
    TextureRect, VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::check::MindCheck;
use super::super::kit::widgets::factory;
use super::super::kit::{self, MindLabel, MindScroll, MindTable, styles};

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "map_info" => Some(Box::new(MapInfoDialog)),
        "map_resize" => Some(Box::new(MapResizeDialog)),
        "map_generate" => Some(Box::new(MapGenerateDialog::default())),
        "wave_info" => Some(Box::new(WaveInfoDialog::default())),
        "map_objectives" => Some(Box::new(MapObjectivesDialog::default())),
        "map_locales" => Some(Box::new(MapLocalesDialog)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Shared plumbing
// ---------------------------------------------------------------------------

/// Meta key carrying the action `shown` dispatches on.
const REQUEST_META: &str = "mind_batch_e_request";
/// `/root/Spine/Ui/UiRoot` (the dialog registry the GDScript used).
const UI_ROOT_PATH: &str = "/root/Spine/Ui/UiRoot";
/// Ninepatch split of `Tex.button` (`MindThemeBuilder._button_nine_patch`).
const BUTTON_SPLIT: f32 = 12.0;

// -- requests ---------------------------------------------------------------

/// Re-runs `shown` synchronously with `request` recorded.
fn request_refresh(node: &Gd<MindDialog>, request: &str) {
    let mut target = node.clone();
    target.set_meta(REQUEST_META, &GString::from(request).to_variant());
    let _ = target.call("refresh_controller", &[]);
}

/// Deferred [`request_refresh`] for callbacks that run inside a `MindUi` borrow.
fn request_refresh_deferred(node: &Gd<MindDialog>, request: &str) {
    let mut target = node.clone();
    target.set_meta(REQUEST_META, &GString::from(request).to_variant());
    let _ = target.call_deferred("refresh_controller", &[]);
}

/// Reads and clears the pending request (`""` when none is queued).
fn take_request(dialog: &mut MindDialog) -> String {
    let mut node = dialog.to_gd();
    if !node.has_meta(REQUEST_META) {
        return String::new();
    }
    let request = variant_text(&node.get_meta(REQUEST_META));
    node.remove_meta(REQUEST_META);
    request
}

// -- Variant coercion -------------------------------------------------------

/// GDScript `str(value)` for a variant (`<null>` for null, like Godot).
fn variant_text(value: &Variant) -> String {
    value
        .try_to::<GString>()
        .map(|text| text.to_string())
        .unwrap_or_else(|_| value.stringify().to_string())
}

/// GDScript `int(value)` for a variant.
fn as_int(value: Option<Variant>) -> i64 {
    let Some(value) = value else {
        return 0;
    };
    if let Ok(number) = value.try_to::<i64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<f64>() {
        return number as i64;
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return i64::from(flag);
    }
    0
}

/// GDScript `float(value)` for a variant.
fn as_float(value: Option<Variant>) -> f64 {
    let Some(value) = value else {
        return 0.0;
    };
    if let Ok(number) = value.try_to::<f64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number as f64;
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return if flag { 1.0 } else { 0.0 };
    }
    0.0
}

/// GDScript `bool(value)` for a variant.
fn as_bool(value: Option<Variant>) -> bool {
    let Some(value) = value else {
        return false;
    };
    value.try_to::<bool>().unwrap_or(false)
}

fn dict_text(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| variant_text(&value))
        .unwrap_or_default()
}

fn dict_int(dict: &VarDictionary, key: &str) -> i64 {
    as_int(dict.get(key))
}

fn dict_float(dict: &VarDictionary, key: &str, fallback: f32) -> f32 {
    match dict.get(key) {
        Some(value) => as_float(Some(value)) as f32,
        None => fallback,
    }
}

fn dict_bool(dict: &VarDictionary, key: &str) -> bool {
    as_bool(dict.get(key))
}

fn dict_array(dict: &VarDictionary, key: &str) -> VarArray {
    match dict.get(key) {
        Some(value) => value.try_to::<VarArray>().unwrap_or_default(),
        None => VarArray::new(),
    }
}

fn dict_dict(dict: &VarDictionary, key: &str) -> VarDictionary {
    match dict.get(key) {
        Some(value) => value.try_to::<VarDictionary>().unwrap_or_default(),
        None => VarDictionary::new(),
    }
}

/// The dictionary rows of a `VarArray` (non-dictionary entries dropped).
fn dict_rows(array: &VarArray) -> Vec<VarDictionary> {
    array
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

// -- JSON -------------------------------------------------------------------

/// `JSON.parse_string` of a payload (`null` for an empty string).
fn json_value(text: &GString) -> Variant {
    if text.is_empty() {
        return Variant::nil();
    }
    Json::parse_string(text)
}

/// `JSON.parse_string` of a payload (`[]` when it is not an array).
fn json_array(text: &GString) -> VarArray {
    json_value(text).try_to::<VarArray>().unwrap_or_default()
}

/// `JSON.parse_string` of a payload (`{}` when it is not an object).
fn json_object(text: &GString) -> VarDictionary {
    json_value(text)
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// `JSON.stringify(value)`.
fn json_string(value: &Variant) -> GString {
    Json::stringify(value)
}

// -- node metadata ----------------------------------------------------------

fn meta_text(node: &Gd<MindDialog>, key: &str) -> String {
    if !node.has_meta(key) {
        return String::new();
    }
    variant_text(&node.get_meta(key))
}

fn meta_int(node: &Gd<MindDialog>, key: &str, fallback: i64) -> i64 {
    if !node.has_meta(key) {
        return fallback;
    }
    as_int(Some(node.get_meta(key)))
}

fn meta_bool(node: &Gd<MindDialog>, key: &str) -> bool {
    node.has_meta(key) && as_bool(Some(node.get_meta(key)))
}

fn set_meta_text(node: &mut Gd<MindDialog>, key: &str, value: &str) {
    node.set_meta(key, &GString::from(value).to_variant());
}

fn set_meta_int(node: &mut Gd<MindDialog>, key: &str, value: i64) {
    node.set_meta(key, &value.to_variant());
}

fn set_meta_bool(node: &mut Gd<MindDialog>, key: &str, value: bool) {
    node.set_meta(key, &value.to_variant());
}

/// The dictionary stored under `key` (`{}` when absent/not a dictionary).
fn meta_dict(node: &Gd<MindDialog>, key: &str) -> VarDictionary {
    if !node.has_meta(key) {
        return VarDictionary::new();
    }
    node.get_meta(key)
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// The array stored under `key` (`[]` when absent/not an array).
fn meta_array(node: &Gd<MindDialog>, key: &str) -> VarArray {
    if !node.has_meta(key) {
        return VarArray::new();
    }
    node.get_meta(key).try_to::<VarArray>().unwrap_or_default()
}

fn set_meta_dict(node: &mut Gd<MindDialog>, key: &str, value: &VarDictionary) {
    node.set_meta(key, &value.to_variant());
}

fn set_meta_array(node: &mut Gd<MindDialog>, key: &str, value: &VarArray) {
    node.set_meta(key, &value.to_variant());
}

// -- editor facade ----------------------------------------------------------

/// Calls a method on `/root/Spine/MindEditor` (`None` when absent).
fn editor_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut editor = kit::editor()?;
    if !editor.has_method(method) {
        return None;
    }
    Some(editor.call(method, args))
}

/// A JSON-returning editor method (`None` when absent or not a string).
fn editor_json(method: &str, args: &[Variant]) -> Option<GString> {
    editor_call(method, args)?.try_to::<GString>().ok()
}

/// `_read_rows()`-style editor payloads as dictionary rows.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn editor_dict_rows(method: &str) -> Vec<VarDictionary> {
    editor_json(method, &[])
        .map(|text| dict_rows(&json_array(&text)))
        .unwrap_or_default()
}

/// The raw array payload of an editor method.
fn editor_array(method: &str) -> VarArray {
    editor_json(method, &[])
        .map(|text| json_array(&text))
        .unwrap_or_default()
}

/// The raw array payload of a one-argument editor method.
fn editor_array_arg(method: &str, argument: &str) -> VarArray {
    editor_json(method, &[GString::from(argument).to_variant()])
        .map(|text| json_array(&text))
        .unwrap_or_default()
}

/// Writes a JSON payload through a `set_*_json` editor method.
fn editor_write(method: &str, json: &GString) -> bool {
    editor_call(method, &[json.clone().to_variant()])
        .map(|value| as_bool(Some(value)))
        .unwrap_or(false)
}

/// `MindEditor.tags()` (`_tags()`).
fn editor_tags() -> VarDictionary {
    editor_call("tags", &[])
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// `MindEditor.set_tag` (`_set_tag`).
fn editor_set_tag(key: &str, value: &str) {
    let _ = editor_call(
        "set_tag",
        &[
            GString::from(key).to_variant(),
            GString::from(value).to_variant(),
        ],
    );
}

/// `MindEditor.palette_teams()` rows.
fn palette_teams() -> Vec<VarDictionary> {
    editor_call("palette_teams", &[])
        .and_then(|value| value.try_to::<VarArray>().ok())
        .map(|array| dict_rows(&array))
        .unwrap_or_default()
}

/// The base teams' display names (`showTeamSelect`).
fn palette_team_names() -> Vec<String> {
    palette_teams()
        .iter()
        .map(|row| dict_text(row, "name"))
        .collect()
}

/// `_team_id(name)` for `MapObjectivesDialog` (the row's `id` field).
fn team_id_by_name(name: &str) -> i64 {
    palette_teams()
        .iter()
        .find(|row| dict_text(row, "name") == name)
        .map(|row| dict_int(row, "id"))
        .unwrap_or(-1)
}

/// `teamName(id)` for `MapObjectivesDialog`.
fn team_name_by_id(id: i64) -> String {
    palette_teams()
        .iter()
        .find(|row| dict_int(row, "id") == id)
        .map(|row| dict_text(row, "name"))
        .unwrap_or_else(|| id.to_string())
}

// -- dialog registry / UI helpers -------------------------------------------

/// A dialog node by manifest name (the `UiRoot` registry, else the tree).
fn dialog_node(name: &str) -> Option<Gd<Node>> {
    if let Some(mut root) = kit::node(UI_ROOT_PATH)
        && root.has_method("dialog")
        && let Ok(node) = root
            .call("dialog", &[GString::from(name).to_variant()])
            .try_to::<Gd<Node>>()
    {
        return Some(node);
    }
    let root = kit::tree_root()?;
    root.find_child(name)
}

/// `MindUi.open_dialog(name, context_json)`.
fn ui_open_dialog(name: &str, context: &VarDictionary) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("open_dialog") {
        return;
    }
    let json = if context.is_empty() {
        GString::new()
    } else {
        json_string(&context.to_variant())
    };
    let _ = ui.call(
        "open_dialog",
        &[GString::from(name).to_variant(), json.to_variant()],
    );
}

/// `MindUi.close_dialog(name)`.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn ui_close_dialog(name: &str) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("close_dialog") {
        return;
    }
    let _ = ui.call("close_dialog", &[GString::from(name).to_variant()]);
}

/// `MindUi.show_info(text)`.
fn ui_show_info(text: &GString) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("show_info") {
        return;
    }
    let _ = ui.call("show_info", &[text.clone().to_variant()]);
}

/// `MindUi.toast(text, kind)`.
fn ui_toast(text: &GString, kind: &str) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("toast") {
        return;
    }
    let _ = ui.call(
        "toast",
        &[text.clone().to_variant(), GString::from(kind).to_variant()],
    );
}

/// `MindUi.show_confirm(text)`.
fn ui_show_confirm(text: &str) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("show_confirm") {
        return;
    }
    let _ = ui.call("show_confirm", &[GString::from(text).to_variant()]);
}

/// `UiRoot.load_and(callback)` (a bare call when the host is absent).
fn ui_root_load_and(callback: Callable) {
    if let Some(mut root) = kit::node(UI_ROOT_PATH)
        && root.has_method("load_and")
    {
        let _ = root.call("load_and", &[callback.to_variant()]);
        return;
    }
    let _ = callback.callv(&VarArray::new());
}

/// The `MindUi.settings_get_text` value.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn settings_text(key: &str, fallback: &str) -> String {
    let Some(mut ui) = kit::ui() else {
        return fallback.to_string();
    };
    if !ui.has_method("settings_get_text") {
        return fallback.to_string();
    }
    ui.call(
        "settings_get_text",
        &[
            GString::from(key).to_variant(),
            GString::from(fallback).to_variant(),
        ],
    )
    .try_to::<GString>()
    .map(|text| text.to_string())
    .unwrap_or_else(|_| fallback.to_string())
}

/// `MindAssets.locales()` (the selectable locale codes).
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn available_locales() -> Vec<String> {
    let Some(assets) = kit::assets() else {
        return vec![String::from("en")];
    };
    let codes = assets.bind().locales();
    let mut out: Vec<String> = codes
        .as_slice()
        .iter()
        .map(|code| code.to_string())
        .collect();
    if out.is_empty() {
        out.push(String::from("en"));
    }
    out
}

/// `MindAssets.locale()` (the live resolved bundle locale).
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn live_locale() -> String {
    let Some(assets) = kit::assets() else {
        return String::new();
    };
    assets.bind().locale().to_string()
}

/// One value from `MindEditor.palette_teams` by row name.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn team_row_name(row: &VarDictionary) -> String {
    dict_text(row, "name")
}

// -- widgets ----------------------------------------------------------------

/// The engine handle of a rich label (text/color/alignment).
fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

/// The control handle of any `Control`-derived node.
fn as_control<N: Inherits<Control>>(node: &Gd<N>) -> Gd<Control> {
    node.clone().upcast()
}

/// `MindWidgets.check(text, checked, listener)`.
fn mind_check(text: &str, checked: bool, listener: Callable) -> Gd<MindCheck> {
    // code-instantiated: the check row is data-driven per settings/field row.
    let mut check = MindCheck::new_alloc();
    check
        .bind_mut()
        .setup(GString::from(text), checked, listener);
    check
}

/// `MindThemeBuilder._button_nine_patch(region)`: a `Tex.button` 9-patch.
fn button_nine_patch(region: &str) -> Gd<StyleBox> {
    let Some(texture) = kit::icon_texture(region) else {
        let mut flat = StyleBoxFlat::new_gd();
        flat.set_bg_color(Color::from_rgba(0.13, 0.15, 0.18, 1.0));
        return flat.upcast();
    };
    // code-instantiated: the atlas 9-patch is resolved at runtime.
    let mut style = StyleBoxTexture::new_gd();
    style.set_texture(&texture);
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        style.set_texture_margin(side, BUTTON_SPLIT);
    }
    style.upcast()
}

/// `_pane_box(fill, border, radius)`: the `Tex.pane` frame used by the pairs.
fn pane_frame(fill: Color, border: Color, radius: f32) -> Gd<StyleBoxFlat> {
    let mut style = MindDialog::pane_box(fill, border, 4.0, radius, -1.0);
    style.set_content_margin_all(0.0);
    style
}

/// A flat `StyleBoxFlat` with a solid fill (`_flat_box`).
fn flat_box(fill: Color) -> Gd<StyleBoxFlat> {
    // code-instantiated: flat style boxes are per-cell presentation.
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(fill);
    style.set_corner_radius_all(0);
    style
}

/// A `LineEdit` carrying a value and a maximum length.
fn text_field(value: &str, max_length: i32) -> Gd<LineEdit> {
    let mut field = factory::field("");
    field.set_text(&GString::from(value));
    field.set_max_length(max_length);
    field
}

/// A `TextEdit` carrying a value (the multi-line `area` fields).
fn text_area(value: &str) -> Gd<TextEdit> {
    // code-instantiated: multi-line fields are data-driven per descriptor.
    let mut area = TextEdit::new_alloc();
    area.set_text(&GString::from(value));
    area.set_line_wrapping_mode(LineWrappingMode::BOUNDARY);
    area
}

/// Truncates to `max_length` characters (GDScript `substr(0, max_length)`).
fn truncate_chars(text: &str, max_length: i32) -> String {
    if max_length < 0 {
        return text.to_string();
    }
    text.chars().take(max_length as usize).collect()
}

/// `text.is_valid_int()`.
fn valid_int(text: &str) -> Option<i64> {
    text.trim().parse::<i64>().ok()
}

/// `text.is_valid_float()`.
fn valid_float(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok()
}

/// `Point2.pack(x, y)`: two 16-bit coordinates in one int.
fn pack_point2(x: i32, y: i32) -> i32 {
    ((x & 0xFFFF) | ((y & 0xFFFF) << 16)) as u32 as i32
}

/// `Point2.x/y` unpack: sign-extends the low half for `x`.
fn unpack_point2(packed: i32) -> (i32, i32) {
    let x = (packed << 16) >> 16;
    let y = packed >> 16;
    (x, y)
}

/// A modal overlay scrim + frame added over the dialog (`_show_overlay`).
struct Overlay {
    #[allow(dead_code)]
    // ported surface; not reachable from the native call graph (kept for parity)
    layer: Gd<ColorRect>,
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    frame_box: Gd<VBoxContainer>,
    scroll: Gd<MindScroll>,
}

impl Overlay {
    /// Adds a content node inside the overlay's scroll pane.
    fn add_content<N: Inherits<Control>>(&self, content: &Gd<N>) {
        let mut scroll: Gd<Node> = self.scroll.clone().upcast();
        let control = as_control(content);
        scroll.add_child(&control);
    }

    /// `_close_overlay()`.
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    fn close(&mut self) {
        if self.layer.is_instance_valid() {
            self.layer.queue_free();
        }
    }
}

/// Builds the modal overlay (`_show_overlay(title, content, min_w, min_h)`).
fn begin_overlay(
    dialog: &mut MindDialog,
    name: &str,
    title_text: &str,
    min_w: f32,
    min_h: f32,
    scrim_color: Color,
) -> Option<Overlay> {
    if let Some(mut existing) = dialog.base().get_node_or_null(name) {
        existing.queue_free();
    }
    // code-instantiated: per-action popup panels have no static scene.
    let mut layer = ColorRect::new_alloc();
    layer.set_name(name);
    layer.set_color(scrim_color);
    layer.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    layer.set_mouse_filter(MouseFilter::STOP);
    let layer_for_dismiss = layer.clone();
    let dismiss = Callable::from_fn("batch_e_overlay_dismiss", move |args: &[&Variant]| {
        let Some(event) = args.first() else {
            return;
        };
        let Ok(event) = event.try_to::<Gd<InputEvent>>() else {
            return;
        };
        let Ok(mouse) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !mouse.is_pressed() {
            return;
        }
        let mut layer = layer_for_dismiss.clone();
        layer.queue_free();
    });
    layer.connect("gui_input", &dismiss);
    let mut center = CenterContainer::new_alloc();
    center.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    center.set_mouse_filter(MouseFilter::IGNORE);
    // code-instantiated: the overlay frame is rebuilt per open.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override(
        "panel",
        &pane_frame(
            Color::from_rgba(0.0, 0.0, 0.0, 0.95),
            styles::PANE_OUTLINE_GRAY,
            4.0,
        ),
    );
    center.add_child(&panel);
    let mut margin = MarginContainer::new_alloc();
    margin.add_theme_constant_override("margin_left", 12);
    margin.add_theme_constant_override("margin_right", 12);
    margin.add_theme_constant_override("margin_top", 8);
    margin.add_theme_constant_override("margin_bottom", 8);
    panel.add_child(&margin);
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 6);
    margin.add_child(&box_);
    let mut title_row = HBoxContainer::new_alloc();
    let mut heading = factory::label(title_text);
    heading.set_h_size_flags(SizeFlags::EXPAND_FILL);
    title_row.add_child(&as_control(&heading));
    let mut close_button = factory::glyph_button("cancel", "X", "clearNonei");
    close_button.set_custom_minimum_size(Vector2::new(36.0, 36.0));
    let layer_for_close = layer.clone();
    let close_callable = Callable::from_fn("batch_e_overlay_close", move |_args: &[&Variant]| {
        let mut layer = layer_for_close.clone();
        layer.queue_free();
    });
    close_button.connect("pressed", &close_callable);
    title_row.add_child(&close_button);
    box_.add_child(&title_row);
    let mut scroll = MindScroll::new_alloc();
    scroll.bind_mut().set_scrolling_disabled(true, false);
    let mut scroll_control = as_control(&scroll);
    scroll_control.set_custom_minimum_size(Vector2::new(min_w, min_h));
    scroll_control.set_v_size_flags(SizeFlags::EXPAND_FILL);
    // A `ScrollContainer` renders only its first child; the caller adds one
    // content node through [`Overlay::add_content`].
    box_.add_child(&scroll_control);
    layer.add_child(&center);
    dialog.base_mut().add_child(&as_control(&layer));
    Some(Overlay {
        layer,
        frame_box: box_,
        scroll,
    })
}

/// Removes a named overlay (`_close_overlay`).
fn close_overlay(dialog: &mut MindDialog, name: &str) {
    if let Some(mut existing) = dialog.base().get_node_or_null(name) {
        existing.queue_free();
    }
}

// ---------------------------------------------------------------------------
// MapInfoDialog (`map_info`, `MapInfoDialog.java`)
// ---------------------------------------------------------------------------

/// `MapInfoDialog.setup` route grid: label key, icon alias, target dialog;
/// `wide` spans both columns (`Icon.fileCode`'s 460px row).
const MAP_INFO_ROUTES: [(&str, &str, &str, bool); 7] = [
    ("@editor.rules", "list", "rules", false),
    ("@editor.waves", "units", "wave_info", false),
    ("@editor.objectives", "info", "map_objectives", false),
    ("@editor.generation", "terrain", "map_generate", false),
    ("@editor.locales", "file-text", "map_locales", false),
    ("@editor.worldprocessors", "logic", "map_processors", false),
    ("@asset.title", "tags", "map_assets", true),
];

/// Guard so the `custom_rules` `rules_edited` signal binds exactly once.
const MAP_INFO_RULES_META: &str = "mind_map_info_rules_bound";
/// Set on the `map_generate` dialog by [`MapInfoDialog`]: `MapGenerateDialog(false)`.
const GEN_APPLIED_META: &str = "mind_map_generate_applied";

/// `@editor.mapinfo` — the map-name/description/author fields and the seven
/// routes into the other editor dialogs (`MapInfoDialog.java:33-126`).
pub struct MapInfoDialog;

impl DialogController for MapInfoDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.mapinfo");
        dialog.should_pause = false;
        dialog.add_close_button(210.0);
        // `cont.pane(...)`: the pane is the body, so the shell must not scroll.
        if let Some(mut body) = dialog.body() {
            body.set_vertical_scroll_mode(ScrollMode::DISABLED);
        }
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let _ = take_request(dialog);
        self.build(dialog);
    }
}

impl MapInfoDialog {
    /// `MapInfoDialog.setup`: the field table inside a pane, then the route
    /// grid inside one `Tex.button` frame (2-column / wide rows).
    fn build(&mut self, dialog: &mut MindDialog) {
        dialog.clear_content();
        let mut root = dialog.content_table();
        // `cont.pane(t -> ...).growX().scrollX(false)`.
        // code-instantiated: the pane wraps the runtime-built field table.
        let mut pane = MindScroll::new_alloc();
        pane.bind_mut().set_scrolling_disabled(true, false);
        let mut inner = pane.bind_mut().content();
        {
            let mut inner_bound = inner.bind_mut();
            inner_bound.grow_horizontal();
            inner_bound.set_table_top(true);
            // `t.margin(16f)`.
            inner_bound.margin(16.0);
            inner_bound.defaults().pad_top = 15.0;
        }
        let mut pane_control = as_control(&pane);
        pane_control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let mut inner_control = as_control(&inner);
        inner_control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        root.bind_mut().add(&pane).grow();
        self.add_field(
            dialog,
            &mut inner,
            "@editor.mapname",
            "name",
            50,
            "@unknown",
            false,
            0.0,
        );
        self.add_field(
            dialog,
            &mut inner,
            "@editor.description",
            "description",
            1000,
            "",
            true,
            15.0,
        );
        self.add_field(
            dialog,
            &mut inner,
            "@editor.author",
            "author",
            50,
            "@unknown",
            false,
            15.0,
        );
        // code-instantiated: `t.table(Tex.button, r -> ...)` frame around the routes.
        let mut frame = PanelContainer::new_alloc();
        let mut box_ = pane_frame(Color::BLACK, styles::PANE_OUTLINE_GRAY, 2.0);
        for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
            box_.set_content_margin(side, 12.0);
        }
        frame.add_theme_stylebox_override("panel", &box_);
        let mut grid = factory::table();
        frame.add_child(&as_control(&grid));
        let mut column = 0usize;
        for (key, icon, action, wide) in MAP_INFO_ROUTES {
            // code-instantiated: one route button per `MapInfoDialog` entry.
            let text = kit::t(key).to_string();
            let mut button = factory::icon_button(
                icon,
                &text,
                "flatt",
                godot::classes::box_container::AlignmentMode::BEGIN,
            );
            inset_icon_row(&mut button, 10.0);
            let target = dialog.to_gd();
            let action_for_press = String::from(action);
            let callable = Callable::from_fn("map_info_route", move |_args: &[&Variant]| {
                open_map_info_route(&target, &action_for_press);
            });
            button.connect("pressed", &callable);
            {
                let mut grid_bound = grid.bind_mut();
                let cell = grid_bound.add(&button);
                let cell = if wide {
                    cell.width(460.0)
                } else {
                    cell.width(230.0)
                };
                let cell = cell.height(60.0);
                let _ = cell.colspan(if wide { 2 } else { 1 });
            }
            if wide {
                grid.bind_mut().row();
                column = 0;
            } else {
                column += 1;
                if column.is_multiple_of(2) {
                    grid.bind_mut().row();
                }
            }
        }
        if !column.is_multiple_of(2) {
            grid.bind_mut().row();
        }
        root.bind_mut().add(&frame).colspan(2);
        root.bind_mut().row();
    }

    /// One label + field row (`t.add(key).padRight(8).left(); t.field(...)`).
    #[allow(clippy::too_many_arguments)]
    fn add_field(
        &mut self,
        dialog: &mut MindDialog,
        table: &mut Gd<MindTable>,
        label_key: &str,
        tag: &str,
        max_length: i32,
        placeholder: &str,
        multiline: bool,
        label_pad_top: f32,
    ) {
        let label = factory::label(&kit::t(label_key).to_string());
        {
            let mut bound = table.bind_mut();
            let _ = bound
                .add(&label)
                .left()
                .pad_right(8.0)
                .pad_top(label_pad_top);
        }
        let tags = editor_tags();
        let current = tags
            .get(tag)
            .map(|value| variant_text(&value))
            .unwrap_or_default();
        if multiline {
            // code-instantiated: the description is the only multi-line field.
            let mut area = text_area(&current);
            area.set_custom_minimum_size(Vector2::new(400.0, 140.0));
            let area_for_edit = area.clone();
            let tag_for_edit = String::from(tag);
            let callable = Callable::from_fn("map_info_area", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| value.to::<GString>().to_string())
                    .unwrap_or_default();
                let text = truncate_chars(&text, max_length);
                if text != area_for_edit.get_text().to_string() {
                    let mut area = area_for_edit.clone();
                    area.set_text(&GString::from(text.as_str()));
                }
                editor_set_tag(&tag_for_edit, &text);
            });
            area.connect("text_changed", &callable);
            let _ = table.bind_mut().add(&area).width(400.0).height(140.0);
        } else {
            let placeholder_text = if placeholder.is_empty() {
                String::new()
            } else {
                kit::t(placeholder).to_string()
            };
            let mut field = factory::field(&placeholder_text);
            field.set_text(&GString::from(current.as_str()));
            field.set_max_length(max_length);
            field.set_custom_minimum_size(Vector2::new(400.0, 55.0));
            let tag_for_edit = String::from(tag);
            let callable = Callable::from_fn("map_info_field", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| value.to::<GString>().to_string())
                    .unwrap_or_default();
                editor_set_tag(&tag_for_edit, &text);
            });
            field.connect("text_changed", &callable);
            let _ = table.bind_mut().add(&field).width(400.0).height(55.0);
        }
        table.bind_mut().row();
        let _ = dialog;
    }
}

/// `.marginLeft(10f)`: the icon row starts 10 px in from the button edge.
fn inset_icon_row(button: &mut Gd<Button>, margin: f32) {
    for child in button.get_children().iter_shared() {
        if let Ok(row) = child.clone().try_cast::<HBoxContainer>() {
            let mut row = row;
            row.set_offset(Side::LEFT, margin);
        }
    }
}

/// `MapInfoDialog.setup` routes (`_open_route`): `rules` edits
/// `state.rules` via the Custom Rules dialog; the rest open their dialog.
fn open_map_info_route(dialog: &Gd<MindDialog>, action: &str) {
    if action == "rules" {
        open_map_info_rules(dialog);
        return;
    }
    if action == "map_generate" {
        // From map info the filters are written to the `genfilters` tag
        // (`MapGenerateDialog(false)`), not applied to the editor.
        if let Some(mut generate) = dialog_node("map_generate") {
            generate.set_meta(GEN_APPLIED_META, &false.to_variant());
            let _ = generate.call_deferred("refresh_controller", &[]);
        }
    }
    ui_open_dialog(action, &VarDictionary::new());
    close_dialog(dialog);
}

/// `ruleInfo.show(Vars.state.rules, () -> Vars.state.rules = new Rules())`:
/// the working rules live in the editor's `rules` tag.
fn open_map_info_rules(dialog: &Gd<MindDialog>) {
    let tags = editor_tags();
    let current = tags
        .get("rules")
        .map(|value| variant_text(&value))
        .map(|text| json_object(&GString::from(text.as_str())))
        .unwrap_or_default();
    let reset = crate::ui::kit::campaign()
        .and_then(|mut campaign| {
            if !campaign.has_method("get_rules_json") {
                return None;
            }
            campaign
                .call("get_rules_json", &[])
                .try_to::<GString>()
                .ok()
        })
        .map(|text| json_object(&text))
        .unwrap_or_default();
    let mut context = VarDictionary::new();
    context.set("rules", &current.to_variant());
    context.set("reset", &reset.to_variant());
    context.set("show_rule_edit", &true.to_variant());
    ui_open_dialog("custom_rules", &context);
    connect_rules_edited(dialog);
    close_dialog(dialog);
}

/// Connects `custom_rules.rules_edited(json)` to the `rules` tag.
fn connect_rules_edited(dialog: &Gd<MindDialog>) {
    if dialog.has_meta(MAP_INFO_RULES_META) {
        return;
    }
    let Some(mut rules) = dialog_node("custom_rules") else {
        return;
    };
    if !rules.has_signal("rules_edited") {
        return;
    }
    let callable = Callable::from_fn("map_info_rules_edited", move |args: &[&Variant]| {
        let Some(json) = args.first() else {
            return;
        };
        editor_set_tag("rules", &variant_text(json));
    });
    rules.connect("rules_edited", &callable);
    dialog
        .clone()
        .set_meta(MAP_INFO_RULES_META, &true.to_variant());
}

/// `_close_pressed`: closes through `MindUi` (stack + pause governor).
fn close_dialog(dialog: &Gd<MindDialog>) {
    let mut node = dialog.clone();
    let _ = node.call("close_pressed", &[]);
}

// ---------------------------------------------------------------------------
// MapResizeDialog (`map_resize`, `MapResizeDialog.java`)
// ---------------------------------------------------------------------------

/// `MapResizeDialog.{minSize, maxSize}`.
const RESIZE_MIN_SIZE: i64 = 50;
const RESIZE_MAX_SIZE: i64 = 800;

/// Meta keys holding the working width/height/shift values.
const RESIZE_WIDTH_META: &str = "mind_resize_width";
const RESIZE_HEIGHT_META: &str = "mind_resize_height";
const RESIZE_SHIFT_X_META: &str = "mind_resize_shift_x";
const RESIZE_SHIFT_Y_META: &str = "mind_resize_shift_y";

/// `@editor.resizemap` — the four fields with Java's filters and the
/// `@cancel`/`@ok` buttons calling `MindEditor.resize_map(w, h, sx, sy)`.
pub struct MapResizeDialog;

impl DialogController for MapResizeDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.resizemap");
        dialog.should_pause = false;
        dialog.clear_buttons();
        let cancel = kit::t("@cancel").to_string();
        let close = dialog.to_gd().callable("close_pressed");
        dialog.add_button(&cancel, Some(close), "", 200.0);
        let ok = kit::t("@ok").to_string();
        let target = dialog.to_gd();
        let apply = Callable::from_fn("map_resize_ok", move |_args: &[&Variant]| {
            resize_apply(&target);
        });
        dialog.add_button(&ok, Some(apply), "", 200.0);
        let mut node = dialog.to_gd();
        set_meta_int(&mut node, RESIZE_SHIFT_X_META, 0);
        set_meta_int(&mut node, RESIZE_SHIFT_Y_META, 0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let _ = take_request(dialog);
        let mut node = dialog.to_gd();
        let status = editor_call("status", &[])
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let width = match status.get("width") {
            Some(value) => as_int(Some(value)),
            None => RESIZE_MIN_SIZE,
        };
        let height = match status.get("height") {
            Some(value) => as_int(Some(value)),
            None => RESIZE_MIN_SIZE,
        };
        set_meta_int(&mut node, RESIZE_WIDTH_META, width);
        set_meta_int(&mut node, RESIZE_HEIGHT_META, height);
        build_resize_body(dialog);
    }
}

/// `shown(() -> { cont.clear(); width = editor.width(); ... })`.
fn build_resize_body(dialog: &mut MindDialog) {
    dialog.clear_content();
    let mut root = dialog.content_table();
    // `cont.table(table -> ...)` with no grow: the label/field column is
    // content-sized and centred.
    let mut column = VBoxContainer::new_alloc();
    root.bind_mut().add(&column).pad(4.0);
    let node = dialog.to_gd();
    let width = meta_int(&node, RESIZE_WIDTH_META, RESIZE_MIN_SIZE);
    let height = meta_int(&node, RESIZE_HEIGHT_META, RESIZE_MIN_SIZE);
    let shift_x = meta_int(&node, RESIZE_SHIFT_X_META, 0);
    let shift_y = meta_int(&node, RESIZE_SHIFT_Y_META, 0);
    column.add_child(&resize_size_row("width", width, true));
    column.add_child(&resize_size_row("height", height, false));
    column.add_child(&resize_shift_row("editor.shiftx", shift_x, true));
    column.add_child(&resize_shift_row("editor.shifty", shift_y, false));
}

/// One size row (`table.field(...).valid(canParsePositiveInt && in range)`).
fn resize_size_row(label_key: &str, value: i64, horizontal: bool) -> Gd<Control> {
    // code-instantiated: one label+field row per resize value.
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 8);
    let mut label = factory::label(&kit::t(&format!("@{label_key}")).to_string());
    label.set_custom_minimum_size(Vector2::new(90.0, 0.0));
    row.add_child(&as_control(&label));
    let mut field = text_field(&value.to_string(), 3);
    field.set_custom_minimum_size(Vector2::new(180.0, 60.0));
    let target = resize_dialog_handle();
    let callable = Callable::from_fn("map_resize_size", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| value.to::<GString>().to_string())
            .unwrap_or_default();
        let Some(number) = valid_int(&text) else {
            return;
        };
        if !(RESIZE_MIN_SIZE..=RESIZE_MAX_SIZE).contains(&number) {
            return;
        }
        if let Some(target) = target.clone() {
            let mut node = target;
            set_meta_int(
                &mut node,
                if horizontal {
                    RESIZE_WIDTH_META
                } else {
                    RESIZE_HEIGHT_META
                },
                number,
            );
        }
    });
    field.connect("text_changed", &callable);
    row.add_child(&field);
    row.upcast()
}

/// One shift row (`Strings::canParseInt`, max length 4).
fn resize_shift_row(label_key: &str, value: i64, horizontal: bool) -> Gd<Control> {
    // code-instantiated: one label+field row per scroll shift.
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 8);
    let mut label = factory::label(&kit::t(&format!("@{label_key}")).to_string());
    label.set_custom_minimum_size(Vector2::new(90.0, 0.0));
    row.add_child(&as_control(&label));
    let mut field = text_field(&value.to_string(), 4);
    field.set_custom_minimum_size(Vector2::new(180.0, 60.0));
    let target = resize_dialog_handle();
    let callable = Callable::from_fn("map_resize_shift", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| value.to::<GString>().to_string())
            .unwrap_or_default();
        // Leading `-` stays typeable; the value commits only on a parseable string.
        if text.is_empty() || text == "-" {
            return;
        }
        let Some(number) = valid_int(&text) else {
            return;
        };
        if let Some(target) = target.clone() {
            let mut node = target;
            set_meta_int(
                &mut node,
                if horizontal {
                    RESIZE_SHIFT_X_META
                } else {
                    RESIZE_SHIFT_Y_META
                },
                number,
            );
        }
    });
    field.connect("text_changed", &callable);
    row.add_child(&field);
    row.upcast()
}

/// The dialog node the row callbacks write their values into.
///
/// The rows are built from [`build_resize_body`], which only sees
/// `&mut MindDialog`; the callbacks re-find the dialog by its `MindUi` name so
/// the metadata writes land on the same node.
fn resize_dialog_handle() -> Option<Gd<MindDialog>> {
    dialog_node("map_resize").and_then(|node| node.try_cast::<MindDialog>().ok())
}

/// `cons.get(width, height, shiftX, shiftY)` + `hide()`.
fn resize_apply(dialog: &Gd<MindDialog>) {
    let node = dialog.clone();
    let width = meta_int(&node, RESIZE_WIDTH_META, RESIZE_MIN_SIZE) as i32;
    let height = meta_int(&node, RESIZE_HEIGHT_META, RESIZE_MIN_SIZE) as i32;
    let shift_x = meta_int(&node, RESIZE_SHIFT_X_META, 0) as i32;
    let shift_y = meta_int(&node, RESIZE_SHIFT_Y_META, 0) as i32;
    let _ = editor_call(
        "resize_map",
        &[
            width.to_variant(),
            height.to_variant(),
            shift_x.to_variant(),
            shift_y.to_variant(),
        ],
    );
    close_dialog(dialog);
}

// ---------------------------------------------------------------------------
// MapGenerateDialog (`map_generate`, `MapGenerateDialog.java`)
// ---------------------------------------------------------------------------

/// `MapGenerateDialog.rebuildFilters`: `table(...).width(280f)`.
const GEN_CARD_WIDTH: f32 = 280.0;
/// Header icon buttons (`t.defaults().size(42f)`).
const GEN_HEADER_ICON: f32 = 42.0;
/// `FilterOption.BlockOption` picker: 10 cells per row and a 44 px cell.
const GEN_PICKER_COLUMNS: i32 = 10;
const GEN_PICKER_CELL: f32 = 44.0;
/// `Pal.gray` (`Pal.java:82`) header tint.
const GEN_HEADER_GRAY: Color = Color::from_rgba8(0x45, 0x45, 0x45, 0xff);
/// Working filter stack (`MapGenerateDialog.filters`).
const GEN_FILTERS_META: &str = "mind_gen_filters";
/// `filter_options_json` rows (option descriptors + resolved values).
const GEN_ROWS_META: &str = "mind_gen_rows";
/// Pending standalone logic-code edit target (`pendingCode`).
const GEN_CODE_INDEX_META: &str = "mind_gen_code_index";
const GEN_CODE_FIELD_META: &str = "mind_gen_code_field";
/// Guard so the `logic` `code_edited` signal binds exactly once.
const GEN_CODE_BOUND_META: &str = "mind_gen_code_bound";
/// The one overlay slot of this dialog (`_overlay`).
const GEN_OVERLAY: &str = "GenerateOverlay";
/// Preview/loading node names looked up by the preview callbacks.
const GEN_PREVIEW_NODE: &str = "GeneratePreview";
const GEN_LOADING_NODE: &str = "GenerateLoading";

/// Requests handled by [`MapGenerateDialog::shown`].
const GEN_REQUEST_LIST: &str = "list";
const GEN_REQUEST_PREVIEW: &str = "preview";

/// `@editor.generate` — the filter-card list with per-filter `FilterOption`
/// editors, the live preview and the `@edit`/`@add` menus.
#[derive(Default)]
pub struct MapGenerateDialog {
    /// The filter card column (`filterTable` in the GDScript).
    filter_table: Option<Gd<MindTable>>,
}

impl DialogController for MapGenerateDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.generate");
        dialog.should_pause = false;
        let mut node = dialog.to_gd();
        if !node.has_meta(GEN_APPLIED_META) {
            set_meta_bool(&mut node, GEN_APPLIED_META, true);
        }
        set_meta_int(&mut node, GEN_CODE_INDEX_META, -1);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let request = take_request(dialog);
        self.build_buttons(dialog);
        match request.as_str() {
            GEN_REQUEST_LIST => {
                self.rebuild_filters(dialog);
                self.refresh_preview(dialog);
            }
            GEN_REQUEST_PREVIEW => self.refresh_preview(dialog),
            // `shown(this::setup)`: read the tag filters, then build preview + cards.
            _ => self.setup(dialog),
        }
    }

    fn hidden(&mut self, dialog: &mut MindDialog) {
        close_overlay(dialog, GEN_OVERLAY);
        // `MapInfoDialog.java:93-96`: reset the seed to 0 so it is not written.
        let node = dialog.to_gd();
        if gen_applied(&node) || kit::editor().is_none() {
            return;
        }
        let mut filters = meta_array(&node, GEN_FILTERS_META);
        for index in 0..filters.len() {
            let Ok(mut filter) = filters.at(index).try_to::<VarDictionary>() else {
                continue;
            };
            filter.set("seed", &0i64.to_variant());
            filters.set(index, &filter.to_variant());
        }
        gen_write(&node, &filters);
    }
}

impl MapGenerateDialog {
    /// `buttons.button("@back"…, "@editor.apply"…, randomize/edit/add)`.
    fn build_buttons(&mut self, dialog: &mut MindDialog) {
        dialog.clear_buttons();
        let target = dialog.to_gd();
        let back = kit::t("@back").to_string();
        dialog.add_button(
            &back,
            Some(dialog.to_gd().callable("close_pressed")),
            "left",
            180.0,
        );
        if gen_applied(&target) {
            let text = kit::t("@editor.apply").to_string();
            let apply_target = target.clone();
            let callable = Callable::from_fn("map_gen_apply", move |_args: &[&Variant]| {
                let apply_target = apply_target.clone();
                let run = Callable::from_fn("map_gen_apply_now", move |_args: &[&Variant]| {
                    gen_apply_now(&apply_target);
                });
                ui_root_load_and(run);
            });
            dialog.add_button(&text, Some(callable), "ok", 180.0);
        }
        let randomize = kit::t("@editor.randomize").to_string();
        let randomize_target = target.clone();
        let callable = Callable::from_fn("map_gen_randomize", move |_args: &[&Variant]| {
            gen_randomize_all(&randomize_target);
        });
        dialog.add_button(&randomize, Some(callable), "refresh", 180.0);
        let edit = kit::t("@edit").to_string();
        let edit_target = target.clone();
        let callable = Callable::from_fn("map_gen_edit", move |_args: &[&Variant]| {
            gen_show_edit_menu(&edit_target);
        });
        dialog.add_button(&edit, Some(callable), "edit", 180.0);
        let add = kit::t("@add").to_string();
        let add_target = target;
        let callable = Callable::from_fn("map_gen_add", move |_args: &[&Variant]| {
            gen_show_add_menu(&add_target);
        });
        dialog.add_button(&add, Some(callable), "add", 180.0);
    }

    /// `shown(this::setup)`.
    fn setup(&mut self, dialog: &mut MindDialog) {
        if kit::editor().is_none() {
            return;
        }
        let node = dialog.to_gd();
        let rows = gen_read_rows();
        let mut filters = gen_read_filters(&rows);
        if !gen_applied(&node) {
            // `MapInfoDialog.java:88-90`: randomize on open so the filters are
            // not all the same seed.
            for index in 0..filters.len() {
                let Ok(mut filter) = filters.at(index).try_to::<VarDictionary>() else {
                    continue;
                };
                filter.set("seed", &random_seed().to_variant());
                filters.set(index, &filter.to_variant());
            }
            gen_write(&node, &filters);
        }
        set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
        set_meta_array(&mut node.clone(), GEN_FILTERS_META, &filters);
        self.build(dialog);
        self.refresh_preview(dialog);
    }

    /// `setup()`: `cont.table(t -> { preview | filter pane }).grow()`.
    fn build(&mut self, dialog: &mut MindDialog) {
        if dialog.content().is_none() {
            return;
        }
        dialog.clear_content();
        let mut root = dialog.content_table();
        let mut columns = HBoxContainer::new_alloc();
        columns.set_h_size_flags(SizeFlags::EXPAND_FILL);
        columns.set_v_size_flags(SizeFlags::EXPAND_FILL);
        columns.add_theme_constant_override("separation", 10);
        root.bind_mut().add(&columns).grow_x().grow_y();
        columns.add_child(&build_generate_preview());
        columns.add_child(&build_generate_filter_list(&mut self.filter_table));
        self.rebuild_filters(dialog);
    }

    /// `rebuildFilters()`: one `Tex.pane` card per filter (or `@filters.empty`).
    fn rebuild_filters(&mut self, dialog: &mut MindDialog) {
        let Some(mut list) = self.filter_table.clone() else {
            return;
        };
        list.bind_mut().clear();
        let node = dialog.to_gd();
        let filters = meta_array(&node, GEN_FILTERS_META);
        let rows = meta_array(&node, GEN_ROWS_META);
        if filters.is_empty() {
            // The bundle text carries `[lightgray]` Arc markup: translate it.
            let mut empty = factory::label(&kit::tm("@filters.empty").to_string());
            empty.set_custom_minimum_size(Vector2::new(200.0, 0.0));
            let _ = list.bind_mut().add(&empty).pad(4.0);
            list.bind_mut().row();
            return;
        }
        for index in 0..filters.len() {
            let card = build_generate_card(&node, index as i64, &rows);
            let _ = list.bind_mut().add(&card).pad(2.0);
            list.bind_mut().row();
        }
    }

    /// `update()`: regenerate the preview from the editor grid + tag filters.
    fn refresh_preview(&mut self, dialog: &mut MindDialog) {
        let node = dialog.to_gd();
        gen_refresh_preview(&node);
    }
}

/// The left preview column (`t.stack(preview, ...).uniformX().grow()`).
fn build_generate_preview() -> Gd<Control> {
    // code-instantiated: preview box + loading stack.
    let mut box_ = Control::new_alloc();
    box_.set_h_size_flags(SizeFlags::EXPAND_FILL);
    box_.set_v_size_flags(SizeFlags::EXPAND_FILL);
    box_.set_custom_minimum_size(Vector2::new(320.0, 320.0));
    box_.set_clip_contents(true);
    // code-instantiated: the preview texture is produced by the editor.
    let mut preview = TextureRect::new_alloc();
    preview.set_name(GEN_PREVIEW_NODE);
    preview.set_expand_mode(ExpandMode::IGNORE_SIZE);
    preview.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    preview.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    preview.set_mouse_filter(MouseFilter::IGNORE);
    box_.add_child(&preview);
    // `t.stack(preview, new Stack(){ black8 + Icon.refresh })`: the loading
    // overlay (`generating && !updateEditorOnChange && dragging == null`).
    let mut loading = Control::new_alloc();
    loading.set_name(GEN_LOADING_NODE);
    loading.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    loading.set_mouse_filter(MouseFilter::IGNORE);
    let mut dim = ColorRect::new_alloc();
    dim.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.8));
    dim.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    dim.set_mouse_filter(MouseFilter::IGNORE);
    loading.add_child(&dim);
    let mut spinner = factory::glyph_label("refresh", 48, Color::WHITE);
    spinner.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    spinner.set_horizontal_alignment(HorizontalAlignment::CENTER);
    spinner.set_vertical_alignment(VerticalAlignment::CENTER);
    spinner.set_mouse_filter(MouseFilter::IGNORE);
    loading.add_child(&spinner);
    loading.set_visible(false);
    box_.add_child(&loading);
    box_
}

/// The right filter pane (`t.pane(p -> filterTable = ...).scrollX(false)`).
fn build_generate_filter_list(filter_table: &mut Option<Gd<MindTable>>) -> Gd<Control> {
    // code-instantiated: the card list count is the filter stack size.
    let mut scroll = MindScroll::new_alloc();
    scroll.bind_mut().set_scrolling_disabled(true, false);
    let mut control = as_control(&scroll);
    control.set_h_size_flags(SizeFlags::EXPAND_FILL);
    control.set_v_size_flags(SizeFlags::EXPAND_FILL);
    control.set_custom_minimum_size(Vector2::new(GEN_CARD_WIDTH + 40.0, 320.0));
    let table = scroll.bind_mut().content();
    *filter_table = Some(table);
    control
}

/// One filter card: the gray header row and the option rows.
fn build_generate_card(node: &Gd<MindDialog>, index: i64, rows: &VarArray) -> Gd<Control> {
    let row = rows
        .get(index.max(0) as usize)
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default();
    // code-instantiated: one card per filter; the stack is runtime-sized.
    let mut card = PanelContainer::new_alloc();
    card.add_theme_stylebox_override(
        "panel",
        &pane_frame(Color::BLACK, styles::PANE_OUTLINE_GRAY, 3.0),
    );
    card.set_custom_minimum_size(Vector2::new(GEN_CARD_WIDTH, 0.0));
    card.set_h_size_flags(SizeFlags::FILL);
    let mut body = VBoxContainer::new_alloc();
    body.add_theme_constant_override("separation", 0);
    card.add_child(&body);
    body.add_child(&build_generate_card_header(node, index, &row));
    let mut options = VBoxContainer::new_alloc();
    options.add_theme_constant_override("separation", 2);
    for option in dict_array(&row, "options").iter_shared() {
        let Ok(option) = option.try_to::<VarDictionary>() else {
            continue;
        };
        options.add_child(&build_generate_option(node, index, &option, rows));
    }
    body.add_child(&options);
    card.upcast()
}

/// `c.table(Tex.whiteui, t -> { name + refresh/up/down/copy/remove })`.
fn build_generate_card_header(
    node: &Gd<MindDialog>,
    index: i64,
    row: &VarDictionary,
) -> Gd<Control> {
    let mut header = PanelContainer::new_alloc();
    let mut style = flat_box(GEN_HEADER_GRAY);
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        style.set_content_margin(
            side,
            if side == Side::TOP || side == Side::BOTTOM {
                2.0
            } else {
                4.0
            },
        );
    }
    header.add_theme_stylebox_override("panel", &style);
    // code-instantiated: header buttons are per-filter action rows.
    let mut line = HBoxContainer::new_alloc();
    line.add_theme_constant_override("separation", 0);
    header.add_child(&line);
    let filter_name = dict_text(row, "name");
    let mut label = factory::label(&kit::t(&format!("@filter.{filter_name}")).to_string());
    label.set_custom_minimum_size(Vector2::new(100.0, 0.0));
    rich(&label).set_autowrap_mode(AutowrapMode::WORD_SMART);
    line.add_child(&as_control(&label));
    let mut spacer = factory::space(0.0, 0.0);
    spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
    line.add_child(&spacer);
    line.add_child(&card_icon(
        node,
        index,
        "refresh",
        "@editor.randomize",
        GenAction::Randomize,
    ));
    if index > 0 {
        line.add_child(&card_icon(
            node,
            index,
            "up-open",
            "@editor.moveup",
            GenAction::Move(-1),
        ));
    }
    let count = meta_array(node, GEN_FILTERS_META).len() as i64;
    if index < count - 1 {
        line.add_child(&card_icon(
            node,
            index,
            "down-open",
            "@editor.movedown",
            GenAction::Move(1),
        ));
    }
    line.add_child(&card_icon(
        node,
        index,
        "copy",
        "@editor.copy",
        GenAction::Copy,
    ));
    line.add_child(&card_icon(
        node,
        index,
        "cancel",
        "@waves.remove",
        GenAction::Remove,
    ));
    header.upcast()
}

/// Per-card header actions.
#[derive(Clone, Copy)]
enum GenAction {
    Randomize,
    Move(i64),
    Copy,
    Remove,
}

/// One 42 px header icon button (`t.defaults().size(42f).padLeft(-5f)`).
fn card_icon(
    node: &Gd<MindDialog>,
    index: i64,
    icon_name: &str,
    tooltip_key: &str,
    action: GenAction,
) -> Gd<Button> {
    // code-instantiated: per-filter header actions are data-driven.
    let mut button = factory::glyph_button(icon_name, icon_name, "clearNonei");
    button.set_custom_minimum_size(Vector2::new(GEN_HEADER_ICON, GEN_HEADER_ICON));
    button.set_h_size_flags(SizeFlags::SHRINK_END);
    button.set_tooltip_text(&kit::t(tooltip_key));
    let target = node.clone();
    let callable = Callable::from_fn(
        "map_gen_card_action",
        move |_args: &[&Variant]| match action {
            GenAction::Randomize => gen_randomize_index(&target, index),
            GenAction::Move(direction) => gen_move(&target, index, direction),
            GenAction::Copy => gen_copy(&target, index),
            GenAction::Remove => gen_remove(&target, index),
        },
    );
    button.connect("pressed", &callable);
    button
}

/// `FilterOption.build` dispatch: slider / block / toggle / custom.
fn build_generate_option(
    node: &Gd<MindDialog>,
    index: i64,
    option: &VarDictionary,
    rows: &VarArray,
) -> Gd<Control> {
    match dict_text(option, "kind").as_str() {
        "slider" => gen_slider_row(node, index, option, rows),
        "block" => gen_block_row(node, index, option, rows),
        "toggle" => gen_toggle_row(node, index, option, rows),
        "custom" => gen_custom_row(node, index, option),
        _ => factory::label("?").upcast(),
    }
}

/// The resolved value behind one option field (`_value_of`).
fn gen_option_value(index: i64, field: &str, option: &VarDictionary, rows: &VarArray) -> Variant {
    if let Some(row) = rows
        .get(index.max(0) as usize)
        .and_then(|value| value.try_to::<VarDictionary>().ok())
    {
        let values = dict_dict(&row, "values");
        if let Some(value) = values.get(field) {
            return value;
        }
    }
    let _ = option;
    Variant::nil()
}

fn gen_option_float(index: i64, field: &str, option: &VarDictionary, rows: &VarArray) -> f64 {
    as_float(Some(gen_option_value(index, field, option, rows)))
}

fn gen_option_string(index: i64, field: &str, option: &VarDictionary, rows: &VarArray) -> String {
    let value = gen_option_value(index, field, option, rows);
    if value.get_type() == VariantType::NIL {
        String::new()
    } else {
        variant_text(&value)
    }
}

fn gen_option_bool(index: i64, field: &str, option: &VarDictionary, rows: &VarArray) -> bool {
    as_bool(Some(gen_option_value(index, field, option, rows)))
}

/// `SliderOption.build`: the label/value row stacked on the slider; the change
/// fires on release (`updateEditorOnChange` is false).
fn gen_slider_row(
    node: &Gd<MindDialog>,
    index: i64,
    option: &VarDictionary,
    rows: &VarArray,
) -> Gd<Control> {
    let field = dict_text(option, "field");
    let value = gen_option_float(index, &field, option, rows);
    let integer = dict_bool(option, "integer");
    // code-instantiated: one slider cell per descriptor (runtime row).
    let mut holder = Control::new_alloc();
    holder.set_custom_minimum_size(Vector2::new(0.0, 46.0));
    holder.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut slider = factory::slider(
        dict_float(option, "min", 0.0) as f64,
        dict_float(option, "max", 1.0) as f64,
        dict_float(option, "step", 0.00001) as f64,
    );
    slider.set_name((format!("FilterSlider_{field}")).as_str());
    slider.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    slider.set_offset(Side::TOP, 10.0);
    slider.set_offset(Side::BOTTOM, -10.0);
    slider.set_value(value);
    holder.add_child(&slider);
    let mut overlay = HBoxContainer::new_alloc();
    overlay.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    overlay.set_mouse_filter(MouseFilter::IGNORE);
    let mut label = factory::label(
        &kit::t(&format!("@filter.option.{}", dict_text(option, "name"))).to_string(),
    );
    label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_mouse_filter(MouseFilter::IGNORE);
    overlay.add_child(&as_control(&label));
    let mut readout = factory::label(&auto_fixed(value));
    readout.set_vertical_alignment(VerticalAlignment::CENTER);
    readout.set_mouse_filter(MouseFilter::IGNORE);
    overlay.add_child(&as_control(&readout));
    holder.add_child(&overlay);
    let readout_for_change = readout.clone();
    let changed = Callable::from_fn("map_gen_slider_value", move |args: &[&Variant]| {
        let value = as_float(args.first().map(|value| (*value).clone()));
        rich(&readout_for_change).set_text(&GString::from(auto_fixed(value).as_str()));
    });
    slider.connect("value_changed", &changed);
    let target = node.clone();
    let field_for_drag = field.clone();
    let slider_for_drag = slider.clone();
    let drag = Callable::from_fn("map_gen_slider_drag", move |args: &[&Variant]| {
        let changed = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        if !changed {
            return;
        }
        let slider = slider_for_drag.clone();
        let value = slider.get_value();
        gen_commit_field(&target, index, &field_for_drag, value.to_variant(), integer);
    });
    slider.connect("drag_ended", &drag);
    holder.upcast()
}

/// `BlockOption.build`: the icon button (click -> picker) + the option label.
fn gen_block_row(
    node: &Gd<MindDialog>,
    index: i64,
    option: &VarDictionary,
    rows: &VarArray,
) -> Gd<Control> {
    let field = dict_text(option, "field");
    let block_name = gen_option_string(index, &field, option, rows);
    let predicate = dict_text(option, "predicate");
    // code-instantiated: the block cell is data-driven (filter option value).
    let mut line = HBoxContainer::new_alloc();
    line.add_theme_constant_override("separation", 6);
    let mut button = block_button(&block_name);
    button.set_name((format!("BlockOption_{field}")).as_str());
    button.set_custom_minimum_size(Vector2::new(48.0, 48.0));
    let target = node.clone();
    let option_for_press = option.clone();
    let press = Callable::from_fn("map_gen_block_picker", move |_args: &[&Variant]| {
        gen_show_block_picker(&target, index, &option_for_press);
    });
    button.connect("pressed", &press);
    let target = node.clone();
    let field_for_click = field.clone();
    let block_for_copy = block_name.clone();
    let predicate_for_paste = predicate;
    let gui_input = Callable::from_fn("map_gen_block_clicks", move |args: &[&Variant]| {
        let Some(event) = args.first() else {
            return;
        };
        let Ok(event) = event.try_to::<Gd<InputEvent>>() else {
            return;
        };
        let Ok(mouse) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !mouse.is_pressed() {
            return;
        }
        match mouse.get_button_index() {
            godot::global::MouseButton::MIDDLE => {
                // `button.clicked(KeyCode.mouseMiddle, ...)`: copy the block name.
                DisplayServer::singleton().clipboard_set(&GString::from(block_for_copy.as_str()));
                ui_toast(&kit::t("@copied"), "info");
            }
            godot::global::MouseButton::RIGHT => {
                // `button.clicked(KeyCode.mouseRight, ...)`: paste a valid block.
                let pasted = DisplayServer::singleton()
                    .clipboard_get()
                    .to_string()
                    .trim()
                    .to_string();
                if gen_valid_block(&predicate_for_paste, &pasted) {
                    gen_commit_field(
                        &target,
                        index,
                        &field_for_click,
                        GString::from(pasted.as_str()).to_variant(),
                        false,
                    );
                    request_refresh(&target, GEN_REQUEST_LIST);
                }
            }
            _ => {}
        }
    });
    button.connect("gui_input", &gui_input);
    line.add_child(&button);
    let mut label = factory::label(
        &kit::t(&format!("@filter.option.{}", dict_text(option, "name"))).to_string(),
    );
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    line.add_child(&as_control(&label));
    line.upcast()
}

/// `ToggleOption.build`: the centered check row.
fn gen_toggle_row(
    node: &Gd<MindDialog>,
    index: i64,
    option: &VarDictionary,
    rows: &VarArray,
) -> Gd<Control> {
    let field = dict_text(option, "field");
    let value = gen_option_bool(index, &field, option, rows);
    let text = kit::t(&format!("@filter.option.{}", dict_text(option, "name"))).to_string();
    let target = node.clone();
    let field_for_toggle = field.clone();
    let listener = Callable::from_fn("map_gen_toggle", move |args: &[&Variant]| {
        let pressed = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        gen_commit_field(
            &target,
            index,
            &field_for_toggle,
            pressed.to_variant(),
            false,
        );
    });
    let mut check = mind_check(&text, value, listener);
    check.set_name((format!("FilterToggle_{field}")).as_str());
    check.set_custom_minimum_size(Vector2::new(0.0, 40.0));
    check.upcast()
}

/// The `LogicFilter` code option: `Icon.pencil` button + the option label.
fn gen_custom_row(node: &Gd<MindDialog>, index: i64, option: &VarDictionary) -> Gd<Control> {
    // code-instantiated: custom actions are descriptor-driven (logic code).
    let mut line = HBoxContainer::new_alloc();
    line.add_theme_constant_override("separation", 6);
    let action = dict_text(option, "action");
    let mut button = factory::glyph_button("pencil", &kit::t("@edit").to_string(), "clearNonei");
    button.set_name((format!("CustomOption_{action}")).as_str());
    button.set_custom_minimum_size(Vector2::new(48.0, 48.0));
    let target = node.clone();
    let action_for_press = action.clone();
    let field = dict_text(option, "field");
    let press = Callable::from_fn("map_gen_custom", move |_args: &[&Variant]| {
        gen_open_custom_action(&target, &action_for_press, index, &field);
    });
    button.connect("pressed", &press);
    line.add_child(&button);
    let mut label = factory::label(
        &kit::t(&format!("@filter.option.{}", dict_text(option, "name"))).to_string(),
    );
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    line.add_child(&as_control(&label));
    line.upcast()
}

/// The current value's `uiIcon` button (`b.image(supplier.get().uiIcon)`).
fn block_button(block_name: &str) -> Gd<Button> {
    if block_name == "air" {
        let mut air = factory::glyph_button("cancel", &kit::t("@none").to_string(), "clearNonei");
        air.set_expand_icon(true);
        return air;
    }
    let ui_region = format!("block-{block_name}-ui");
    let full_region = format!("block-{block_name}-full");
    let mut button =
        factory::image_button_first(&[ui_region.as_str(), full_region.as_str()], "clearNonei");
    if button.get_button_icon().is_none() {
        button.set_text(&GString::from(block_name));
    }
    button.set_expand_icon(true);
    button.add_theme_constant_override("icon_max_width", 36);
    button.set_tooltip_text(&kit::t(&format!("@block.{block_name}.name")));
    button
}

/// `Strings.autoFixed(value, 2)`.
fn auto_fixed(value: f64) -> String {
    format!("{value:.2}")
}

/// `randi() % 999999999` (`_random_seed`).
fn random_seed() -> i64 {
    godot::global::randi() % 999_999_999
}

/// `MapGenerateDialog.filters`: the tag stack, or the class defaults.
fn gen_read_filters(rows: &VarArray) -> VarArray {
    if let Some(text) = editor_json("filters_json", &[]) {
        let parsed = json_value(&text);
        if let Ok(array) = parsed.try_to::<VarArray>() {
            return array;
        }
    }
    let mut out = VarArray::new();
    for row in dict_rows(rows) {
        let values = dict_dict(&row, "values");
        if !values.is_empty() {
            out.push(&values.duplicate_deep().to_variant());
        }
    }
    out
}

/// `filter_options_json` rows.
fn gen_read_rows() -> VarArray {
    editor_json("filter_options_json", &[])
        .map(|text| json_array(&text))
        .unwrap_or_default()
}

/// `_read_types()` (`Maps.allFilterTypes` rows for the add picker).
fn gen_read_types() -> VarArray {
    editor_json("filter_types_json", &[])
        .map(|text| json_array(&text))
        .unwrap_or_default()
}

/// `_write(filters)`: the `set_filters_json` tag write.
fn gen_write(node: &Gd<MindDialog>, filters: &VarArray) {
    set_meta_array(&mut node.clone(), GEN_FILTERS_META, filters);
    let _ = editor_write("set_filters_json", &json_string(&filters.to_variant()));
}

/// The working filters from metadata.
fn gen_filters(node: &Gd<MindDialog>) -> VarArray {
    meta_array(node, GEN_FILTERS_META)
}

/// `_commit_field`: one value into the working filters + the mirror rows.
fn gen_commit_field(node: &Gd<MindDialog>, index: i64, field: &str, value: Variant, integer: bool) {
    let mut filters = gen_filters(node);
    if index < 0 || index >= filters.len() as i64 {
        return;
    }
    let Ok(mut filter) = filters.at(index as usize).try_to::<VarDictionary>() else {
        return;
    };
    // `if(integer && value instanceof Float) value = Mathf.round(value)`.
    let mut stored = value.clone();
    if integer && let Ok(number) = value.try_to::<f64>() {
        stored = (number.round() as i64).to_variant();
    }
    filter.set(field, &stored);
    filters.set(index as usize, &filter.to_variant());
    let mut rows = meta_array(node, GEN_ROWS_META);
    if index < rows.len() as i64
        && let Ok(mut row) = rows.at(index as usize).try_to::<VarDictionary>()
    {
        let mut values = dict_dict(&row, "values");
        values.set(field, &stored);
        row.set("values", &values.to_variant());
        rows.set(index as usize, &row.to_variant());
        set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
    }
    gen_write(node, &filters);
    gen_refresh_preview(node);
}

/// The `@editor.randomize` toolbar button: `for(filter : filters) filter.randomize()`.
fn gen_randomize_all(node: &Gd<MindDialog>) {
    let mut filters = gen_filters(node);
    for index in 0..filters.len() {
        let Ok(mut filter) = filters.at(index).try_to::<VarDictionary>() else {
            continue;
        };
        filter.set("seed", &random_seed().to_variant());
        filters.set(index, &filter.to_variant());
    }
    gen_write(node, &filters);
    gen_refresh_preview(node);
}

/// The per-card refresh (`filter.randomize(); update()`).
fn gen_randomize_index(node: &Gd<MindDialog>, index: i64) {
    let mut filters = gen_filters(node);
    if index < 0 || index >= filters.len() as i64 {
        return;
    }
    let Ok(mut filter) = filters.at(index as usize).try_to::<VarDictionary>() else {
        return;
    };
    filter.set("seed", &random_seed().to_variant());
    filters.set(index as usize, &filter.to_variant());
    gen_write(node, &filters);
    gen_refresh_preview(node);
}

/// The per-card up/down swap (`filters.swap(idx, idx +/- 1)`).
fn gen_move(node: &Gd<MindDialog>, index: i64, direction: i64) {
    let other = index + direction;
    let mut filters = gen_filters(node);
    let count = filters.len() as i64;
    if index < 0 || other < 0 || index >= count || other >= count {
        return;
    }
    let first = filters.at(index as usize);
    let second = filters.at(other as usize);
    filters.set(index as usize, &second);
    filters.set(other as usize, &first);
    gen_write(node, &filters);
    let rows = gen_read_rows();
    set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
    request_refresh(node, GEN_REQUEST_LIST);
}

/// The per-card copy (`copy.randomize(); filters.insert(idx + 1, copy)`).
fn gen_copy(node: &Gd<MindDialog>, index: i64) {
    let mut filters = gen_filters(node);
    if index < 0 || index >= filters.len() as i64 {
        return;
    }
    let Ok(mut copy) = filters.at(index as usize).try_to::<VarDictionary>() else {
        return;
    };
    copy.set("seed", &random_seed().to_variant());
    filters.insert(index as usize + 1, &copy.to_variant());
    gen_write(node, &filters);
    let rows = gen_read_rows();
    set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
    request_refresh(node, GEN_REQUEST_LIST);
}

/// The per-card remove (`filters.remove(filter)`).
fn gen_remove(node: &Gd<MindDialog>, index: i64) {
    let mut filters = gen_filters(node);
    if index < 0 || index >= filters.len() as i64 {
        return;
    }
    filters.remove(index as usize);
    gen_write(node, &filters);
    let rows = gen_read_rows();
    set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
    request_refresh(node, GEN_REQUEST_LIST);
}

/// `buttons.button("@editor.apply", ...)`: write the tag, then `apply_filters()`.
fn gen_apply_now(node: &Gd<MindDialog>) {
    if kit::editor().is_none() {
        return;
    }
    let filters = gen_filters(node);
    gen_write(node, &filters);
    let _ = editor_call("apply_filters", &[]);
    close_dialog(node);
}

/// `_refresh_preview()`: regenerate the preview from the editor grid + filters.
fn gen_refresh_preview(node: &Gd<MindDialog>) {
    let Some(mut preview) =
        find_named(node, GEN_PREVIEW_NODE).and_then(|child| child.try_cast::<TextureRect>().ok())
    else {
        return;
    };
    let Some(texture) = editor_call("generate_preview_texture", &[])
        .and_then(|value| value.try_to::<Gd<Texture2D>>().ok())
    else {
        return;
    };
    preview.set_texture(&texture);
    if let Some(mut loading) =
        find_named(node, GEN_LOADING_NODE).and_then(|child| child.try_cast::<Control>().ok())
    {
        loading.set_visible(false);
    }
}

/// `showAdd()`: every `Maps.allFilterTypes` entry (post filters hidden in
/// applied mode) plus the `@filter.defaultores` row.
fn gen_show_add_menu(node: &Gd<MindDialog>) {
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    let applied = gen_applied(node);
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(3);
    grid.add_theme_constant_override("h_separation", 4);
    grid.add_theme_constant_override("v_separation", 4);
    for row in dict_rows(&gen_read_types()) {
        if dict_bool(&row, "post") && applied {
            continue;
        }
        let mut button = icon_text_button(
            &dict_text(&row, "icon"),
            &kit::t(&format!("@filter.{}", dict_text(&row, "name"))).to_string(),
            "",
        );
        button.set_custom_minimum_size(Vector2::new(195.0, 56.0));
        let target = node.clone();
        let class_tag = dict_text(&row, "class");
        let callable = Callable::from_fn("map_gen_add_filter", move |_args: &[&Variant]| {
            gen_add_filter(&target, &class_tag);
        });
        button.connect("pressed", &callable);
        grid.add_child(&button);
    }
    let mut default_ores =
        icon_text_button("", &kit::t("@filter.defaultores").to_string(), "refresh");
    default_ores.set_custom_minimum_size(Vector2::new(195.0, 56.0));
    let target = node.clone();
    let callable = Callable::from_fn("map_gen_default_ores", move |_args: &[&Variant]| {
        gen_add_default_ores(&target);
    });
    default_ores.connect("pressed", &callable);
    grid.add_child(&default_ores);
    let title = kit::t("@add").to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        GEN_OVERLAY,
        &title,
        640.0,
        380.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.7),
    ) {
        overlay.add_content(&grid);
    }
}

/// `showAdd()`'s `filter.randomize(); filters.add(filter);`.
fn gen_add_filter(node: &Gd<MindDialog>, class_tag: &str) {
    if class_tag.is_empty() {
        return;
    }
    let mut filters = gen_filters(node);
    let mut filter = VarDictionary::new();
    filter.set("class", &GString::from(class_tag).to_variant());
    filter.set("seed", &random_seed().to_variant());
    filters.push(&filter);
    gen_write(node, &filters);
    let rows = gen_read_rows();
    set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
    free_named(node, GEN_OVERLAY);
    request_refresh(node, GEN_REQUEST_LIST);
}

/// `@filter.defaultores`: `maps.addDefaultOres(filters)`.
fn gen_add_default_ores(node: &Gd<MindDialog>) {
    if kit::editor().is_none() {
        return;
    }
    let Some(text) = editor_json("default_ores_json", &[]) else {
        return;
    };
    let parsed = json_value(&text);
    let Ok(extra) = parsed.try_to::<VarArray>() else {
        return;
    };
    let mut filters = gen_filters(node);
    for value in extra.iter_shared() {
        filters.push(&value);
    }
    gen_write(node, &filters);
    let rows = gen_read_rows();
    set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
    free_named(node, GEN_OVERLAY);
    request_refresh(node, GEN_REQUEST_LIST);
}

/// `buttons.button("@edit", ...)`: the clipboard/clear/reset rows.
fn gen_show_edit_menu(node: &Gd<MindDialog>) {
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    // code-instantiated: menu rows depend on the clipboard/applied flags.
    let mut rows = VBoxContainer::new_alloc();
    rows.set_custom_minimum_size(Vector2::new(320.0, 0.0));
    let clipboard_empty = DisplayServer::singleton()
        .clipboard_get()
        .to_string()
        .trim()
        .is_empty();
    gen_menu_button(
        node,
        &mut rows,
        "copy.clipboard",
        "copy",
        GenMenu::Copy,
        false,
    );
    gen_menu_button(
        node,
        &mut rows,
        "load.clipboard",
        "download",
        GenMenu::Load,
        clipboard_empty,
    );
    gen_menu_button(node, &mut rows, "clear", "cancel", GenMenu::Clear, false);
    let mut count = 3.0;
    if !gen_applied(node) {
        gen_menu_button(
            node,
            &mut rows,
            "settings.reset",
            "refresh",
            GenMenu::Reset,
            false,
        );
        count += 1.0;
    }
    let title = kit::t("@editor.export").to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        GEN_OVERLAY,
        &title,
        320.0,
        count * 60.0 + 4.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.7),
    ) {
        overlay.add_content(&rows);
    }
}

/// The `@edit` menu actions.
#[derive(Clone, Copy)]
enum GenMenu {
    Copy,
    Load,
    Clear,
    Reset,
}

/// One `@edit` menu row.
fn gen_menu_button(
    node: &Gd<MindDialog>,
    rows: &mut Gd<VBoxContainer>,
    key: &str,
    icon: &str,
    action: GenMenu,
    disabled: bool,
) {
    let mut button = factory::icon_button(
        icon,
        &kit::t(&format!("@{key}")).to_string(),
        "flatt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    button.set_custom_minimum_size(Vector2::new(280.0, 60.0));
    button.set_disabled(disabled);
    let target = node.clone();
    let callable = Callable::from_fn("map_gen_menu", move |_args: &[&Variant]| {
        gen_menu_action(&target, action);
    });
    button.connect("pressed", &callable);
    rows.add_child(&button);
}

/// Runs one `@edit` menu action.
fn gen_menu_action(node: &Gd<MindDialog>, action: GenMenu) {
    match action {
        GenMenu::Copy => {
            // `Core.app.setClipboardText(JsonIO.write(filters))`.
            let filters = gen_filters(node);
            DisplayServer::singleton().clipboard_set(&json_string(&filters.to_variant()));
            free_named(node, GEN_OVERLAY);
            ui_toast(&kit::t("@copied"), "info");
        }
        GenMenu::Load => {
            // `filters.set(JsonIO.read(Seq.class, Core.app.getClipboardText()))`.
            let text = DisplayServer::singleton().clipboard_get();
            let parsed = json_value(&text);
            let Ok(filters) = parsed.try_to::<VarArray>() else {
                free_named(node, GEN_OVERLAY);
                ui_toast(&kit::t("@save.import.invalid"), "error");
                return;
            };
            gen_write(node, &filters);
            let rows = gen_read_rows();
            set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
            free_named(node, GEN_OVERLAY);
            request_refresh(node, GEN_REQUEST_LIST);
        }
        GenMenu::Clear => {
            let filters = VarArray::new();
            gen_write(node, &filters);
            set_meta_array(&mut node.clone(), GEN_ROWS_META, &VarArray::new());
            free_named(node, GEN_OVERLAY);
            request_refresh(node, GEN_REQUEST_LIST);
        }
        GenMenu::Reset => {
            // `filters.set(maps.readFilters(""))`.
            let _ = editor_write("set_filters_json", &GString::new());
            let rows = gen_read_rows();
            let filters = gen_read_filters(&rows);
            set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
            set_meta_array(&mut node.clone(), GEN_FILTERS_META, &filters);
            free_named(node, GEN_OVERLAY);
            request_refresh(node, GEN_REQUEST_LIST);
        }
    }
}

/// `FilterOption.BlockOption` picker: the block grid filtered by the predicate.
fn gen_show_block_picker(node: &Gd<MindDialog>, index: i64, option: &VarDictionary) {
    let predicate = dict_text(option, "predicate");
    let field = dict_text(option, "field");
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    // code-instantiated: picker cells are content rows.
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(GEN_PICKER_COLUMNS);
    grid.add_theme_constant_override("h_separation", 4);
    grid.add_theme_constant_override("v_separation", 4);
    for row in dict_rows(&editor_array_arg("filter_blocks_json", &predicate)) {
        let block_name = dict_text(&row, "name");
        let mut cell = block_button(&block_name);
        cell.set_custom_minimum_size(Vector2::new(GEN_PICKER_CELL, GEN_PICKER_CELL));
        if block_name == "air" {
            cell.set_tooltip_text(&kit::t("@none"));
        }
        let target = node.clone();
        let field_for_pick = field.clone();
        let block_for_pick = block_name.clone();
        let callable = Callable::from_fn("map_gen_pick_block", move |_args: &[&Variant]| {
            gen_commit_field(
                &target,
                index,
                &field_for_pick,
                GString::from(block_for_pick.as_str()).to_variant(),
                false,
            );
            free_named(&target, GEN_OVERLAY);
            // The button icon reflects the new value.
            request_refresh(&target, GEN_REQUEST_LIST);
        });
        cell.connect("pressed", &callable);
        grid.add_child(&cell);
    }
    let title = kit::t(&format!("@filter.option.{}", dict_text(option, "name"))).to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        GEN_OVERLAY,
        &title,
        900.0,
        420.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.7),
    ) {
        overlay.add_content(&grid);
    }
}

/// `_valid_block(predicate, name)`.
fn gen_valid_block(predicate: &str, block_name: &str) -> bool {
    if block_name.is_empty() {
        return false;
    }
    dict_rows(&editor_array_arg("filter_blocks_json", predicate))
        .iter()
        .any(|row| dict_text(row, "name") == block_name)
}

/// `ui.logic.show(code, null, true, code -> ...)`: the shared logic editor in
/// its standalone mode (context `filter` + `code`); it echoes the edited code
/// back through `code_edited`.
fn gen_open_custom_action(node: &Gd<MindDialog>, action: &str, index: i64, field: &str) {
    if action != "logiccode" {
        return;
    }
    if kit::ui().is_none() {
        return;
    }
    let mut meta = node.clone();
    set_meta_int(&mut meta, GEN_CODE_INDEX_META, index);
    set_meta_text(&mut meta, GEN_CODE_FIELD_META, field);
    gen_bind_code_edited(node);
    let rows = meta_array(node, GEN_ROWS_META);
    let option = gen_option_for(node, index, field);
    let code = gen_option_string(index, field, &option, &rows);
    let mut context = VarDictionary::new();
    context.set("filter", &true.to_variant());
    context.set("privileged", &true.to_variant());
    context.set("code", &GString::from(code.as_str()).to_variant());
    ui_open_dialog("logic", &context);
}

/// The option descriptor that owns `field` on filter `index`.
fn gen_option_for(node: &Gd<MindDialog>, index: i64, field: &str) -> VarDictionary {
    let rows = meta_array(node, GEN_ROWS_META);
    if let Some(row) = rows
        .get(index.max(0) as usize)
        .and_then(|value| value.try_to::<VarDictionary>().ok())
    {
        for option in dict_array(&row, "options").iter_shared() {
            let Ok(option) = option.try_to::<VarDictionary>() else {
                continue;
            };
            if dict_text(&option, "field") == field {
                return option;
            }
        }
    }
    VarDictionary::new()
}

/// Binds the `logic` dialog's `code_edited` signal (once).
fn gen_bind_code_edited(node: &Gd<MindDialog>) {
    if node.has_meta(GEN_CODE_BOUND_META) {
        return;
    }
    let Some(mut logic) = dialog_node("logic") else {
        return;
    };
    if !logic.has_signal("code_edited") {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("map_gen_code_edited", move |args: &[&Variant]| {
        let code = args
            .first()
            .map(|value| value.to::<GString>())
            .unwrap_or_default();
        let mut meta = target.clone();
        let index = meta_int(&meta, GEN_CODE_INDEX_META, -1);
        if index < 0 {
            return;
        }
        let field = meta_text(&meta, GEN_CODE_FIELD_META);
        set_meta_int(&mut meta, GEN_CODE_INDEX_META, -1);
        gen_commit_field(&target, index, &field, code.to_variant(), false);
        request_refresh(&target, GEN_REQUEST_LIST);
    });
    logic.connect("code_edited", &callable);
    node.clone()
        .set_meta(GEN_CODE_BOUND_META, &true.to_variant());
}

/// An icon + left-aligned label button (the `@add` picker rows).
fn icon_text_button(icon_region: &str, text_value: &str, glyph: &str) -> Gd<Button> {
    // code-instantiated: icon+label picker rows are data-driven (filter types).
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("defaultt");
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, 8.0);
    row.set_offset(Side::RIGHT, -4.0);
    row.add_theme_constant_override("separation", 6);
    row.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&row);
    if !glyph.is_empty() {
        row.add_child(&factory::glyph_label(glyph, 24, Color::WHITE));
    } else if !icon_region.is_empty() {
        let region = format!("block-{icon_region}-ui");
        if let Some(icon) = factory::content_icon(&region, "block", 28.0) {
            row.add_child(&icon);
        }
    }
    let mut label = factory::label(text_value);
    rich(&label).set_autowrap_mode(AutowrapMode::OFF);
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_horizontal_alignment(HorizontalAlignment::LEFT);
    label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    label.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&as_control(&label));
    button
}

/// `MapGenerateDialog(false)` when opened from the map-info route.
fn gen_applied(node: &Gd<MindDialog>) -> bool {
    if !node.has_meta(GEN_APPLIED_META) {
        return true;
    }
    meta_bool(node, GEN_APPLIED_META)
}

/// The first descendant named `name` (runtime nodes are not scene-owned).
fn find_named(node: &Gd<MindDialog>, name: &str) -> Option<Gd<Node>> {
    let root: Gd<Node> = node.clone().upcast();
    root.find_child_ex(name).owned(false).done()
}

/// Frees the first descendant named `name`.
fn free_named(node: &Gd<MindDialog>, name: &str) {
    if let Some(mut child) = find_named(node, name) {
        child.queue_free();
    }
}

// ---------------------------------------------------------------------------
// WaveInfoDialog (`wave_info`, `WaveInfoDialog.java`)
// ---------------------------------------------------------------------------

/// `SpawnGroup.NEVER` (`i32::MAX`).
const WAVE_NEVER: i64 = 2_147_483_647;
/// `WaveInfoDialog.setup`: the left editor column.
const WAVE_PANEL_WIDTH: f32 = 390.0;
/// `spawner.getSpawns()` display cap (`i >= 20`).
const WAVE_SPAWN_CAP: usize = 20;

/// Working group list (`WaveInfoDialog.groups`).
const WAVE_GROUPS_META: &str = "mind_wave_groups";
/// Expanded group index (`expandedGroup`), -1 for none.
const WAVE_EXPANDED_META: &str = "mind_wave_expanded";
/// 0-based wave search (`search`), -1 for none.
const WAVE_SEARCH_META: &str = "mind_wave_search";
/// Unit-type filter (`filterType`), "" for none.
const WAVE_FILTER_META: &str = "mind_wave_filter";
/// `Sort` enum (`0` begin, `1` health, `2` type).
const WAVE_SORT_META: &str = "mind_wave_sort";
const WAVE_REVERSE_META: &str = "mind_wave_reverse";
/// Pending `list_select` target and the picked value.
const WAVE_PICKER_META: &str = "mind_wave_picker";
const WAVE_PICKED_META: &str = "mind_wave_picked";
const WAVE_PICKER_BOUND_META: &str = "mind_wave_picker_bound";
/// Inline panels (`@edit.menu` / `@waves.sort`).
const WAVE_EDIT_OPEN_META: &str = "mind_wave_edit_open";
const WAVE_SORT_OPEN_META: &str = "mind_wave_sort_open";
/// The graph node name (`WaveGraph` scene instance).
const WAVE_GRAPH_NODE: &str = "WaveGraph";

const WAVE_REQUEST_LIST: &str = "list";
const WAVE_REQUEST_PICKED: &str = "picked";
const WAVE_REQUEST_BUILD: &str = "build";

/// `@waves.title` — the wave group editor plus the `WaveGraph`.
#[derive(Default)]
pub struct WaveInfoDialog {
    /// The group list column (`_list`).
    list: Option<Gd<MindTable>>,
}

impl DialogController for WaveInfoDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@waves.title");
        dialog.should_pause = false;
        dialog.clear_buttons();
        // `addCloseButton()` first, then `@edit.menu` and `waves.random`.
        dialog.add_close_button(210.0);
        let node = dialog.to_gd();
        let edit = kit::t("@edit.menu").to_string();
        let edit_target = node.clone();
        let callable = Callable::from_fn("wave_edit_menu", move |_args: &[&Variant]| {
            let mut meta = edit_target.clone();
            let open = !meta_bool(&meta, WAVE_EDIT_OPEN_META);
            set_meta_bool(&mut meta, WAVE_EDIT_OPEN_META, open);
            set_meta_bool(&mut meta, WAVE_SORT_OPEN_META, false);
            request_refresh(&edit_target, WAVE_REQUEST_BUILD);
        });
        dialog.add_button(&edit, Some(callable), "edit", 250.0);
        let random = kit::t("@waves.random").to_string();
        let random_target = node.clone();
        let callable = Callable::from_fn("wave_random", move |_args: &[&Variant]| {
            if as_bool(editor_call("randomize_waves", &[])) {
                let groups = wave_read_groups();
                let mut meta = random_target.clone();
                set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
                set_meta_int(&mut meta, WAVE_EXPANDED_META, -1);
                request_refresh(&random_target, WAVE_REQUEST_BUILD);
            }
        });
        dialog.add_button(&random, Some(callable), "refresh", 200.0);
        wave_bind_picker(&node);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let request = take_request(dialog);
        match request.as_str() {
            WAVE_REQUEST_LIST => self.rebuild(dialog),
            WAVE_REQUEST_PICKED => {
                wave_apply_picked(&dialog.to_gd());
                self.rebuild(dialog);
            }
            WAVE_REQUEST_BUILD => self.build(dialog),
            // `shown()`: read the groups (vanilla fallback included) and build.
            _ => self.setup(dialog),
        }
    }
}

impl WaveInfoDialog {
    /// `shown()`: `_groups = _read_groups(); _expanded = -1; _commit(); _build()`.
    fn setup(&mut self, dialog: &mut MindDialog) {
        let node = dialog.to_gd();
        let groups = wave_read_groups();
        let mut meta = node.clone();
        set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
        set_meta_int(&mut meta, WAVE_EXPANDED_META, -1);
        set_meta_int(&mut meta, WAVE_SEARCH_META, -1);
        wave_commit(&node);
        self.build(dialog);
    }

    /// `_build()`: the search/list/add column and the wave graph.
    fn build(&mut self, dialog: &mut MindDialog) {
        if dialog.content().is_none() {
            return;
        }
        dialog.clear_content();
        let mut root = dialog.content_table();
        let node = dialog.to_gd();
        let mut row = HBoxContainer::new_alloc();
        row.set_h_size_flags(SizeFlags::EXPAND_FILL);
        row.set_v_size_flags(SizeFlags::EXPAND_FILL);
        root.bind_mut().add(&row).grow_x().grow_y();
        let mut left = VBoxContainer::new_alloc();
        left.set_custom_minimum_size(Vector2::new(WAVE_PANEL_WIDTH, 0.0));
        left.set_h_size_flags(SizeFlags::FILL);
        left.set_v_size_flags(SizeFlags::EXPAND_FILL);
        row.add_child(&left);
        // The search row (`@waves.search` + the unit filter button).
        let mut search = HBoxContainer::new_alloc();
        search.add_child(&factory::glyph_label("zoom", 20, Color::WHITE));
        let mut search_field = factory::field(&kit::t("@waves.search").to_string());
        search_field.set_max_length(8);
        search_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let search_target = node.clone();
        let callable = Callable::from_fn("wave_search", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| value.to::<GString>().to_string())
                .unwrap_or_default();
            let search = valid_int(&text).map(|value| value - 1).unwrap_or(-1);
            set_meta_int(&mut search_target.clone(), WAVE_SEARCH_META, search);
            request_refresh(&search_target, WAVE_REQUEST_LIST);
        });
        search_field.connect("text_changed", &callable);
        search.add_child(&search_field);
        let mut filter_button = factory::icon_button(
            "filter",
            "",
            "emptyi",
            godot::classes::box_container::AlignmentMode::BEGIN,
        );
        filter_button.set_custom_minimum_size(Vector2::new(46.0, 46.0));
        filter_button.set_tooltip_text(&kit::t("@waves.filter"));
        let filter_target = node.clone();
        let callable = Callable::from_fn("wave_filter", move |_args: &[&Variant]| {
            wave_open_picker(
                &filter_target,
                "@waves.filter",
                &wave_unit_names(),
                "@settings.resetKey",
                "filter",
            );
        });
        filter_button.connect("pressed", &callable);
        search.add_child(&filter_button);
        left.add_child(&search);
        // The scrolling group list (`pane(p -> ...)`).
        // code-instantiated: the group list is the runtime spawns table.
        let mut scroll = MindScroll::new_alloc();
        scroll.bind_mut().set_scrolling_disabled(true, false);
        let mut scroll_control = as_control(&scroll);
        scroll_control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        scroll_control.set_v_size_flags(SizeFlags::EXPAND_FILL);
        scroll_control.set_custom_minimum_size(Vector2::new(WAVE_PANEL_WIDTH - 20.0, 260.0));
        let mut list = scroll.bind_mut().content();
        list.bind_mut().grow_horizontal();
        self.list = Some(list);
        left.add_child(&scroll_control);
        // The add + sort action row.
        let mut actions = HBoxContainer::new_alloc();
        let mut add = factory::button(&kit::t("@add").to_string());
        add.set_custom_minimum_size(Vector2::new(0.0, 70.0));
        add.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let add_target = node.clone();
        let callable = Callable::from_fn("wave_add", move |_args: &[&Variant]| {
            wave_open_picker(&add_target, "", &wave_unit_names(), "", "add");
        });
        add.connect("pressed", &callable);
        actions.add_child(&add);
        let mut sort_button = factory::icon_button(
            "filter",
            "",
            "defaulti",
            godot::classes::box_container::AlignmentMode::BEGIN,
        );
        sort_button.set_custom_minimum_size(Vector2::new(64.0, 70.0));
        for (slot, region) in [
            ("normal", "button"),
            ("hover", "button-over"),
            ("pressed", "button-down"),
        ] {
            sort_button.add_theme_stylebox_override(slot, &button_nine_patch(region));
        }
        actions.add_theme_constant_override("separation", 6);
        let sort_target = node.clone();
        let callable = Callable::from_fn("wave_sort_open", move |_args: &[&Variant]| {
            let mut meta = sort_target.clone();
            let open = !meta_bool(&meta, WAVE_SORT_OPEN_META);
            set_meta_bool(&mut meta, WAVE_SORT_OPEN_META, open);
            set_meta_bool(&mut meta, WAVE_EDIT_OPEN_META, false);
            request_refresh(&sort_target, WAVE_REQUEST_BUILD);
        });
        sort_button.connect("pressed", &callable);
        actions.add_child(&sort_button);
        left.add_child(&actions);
        // The inline `@edit.menu` / `@waves.sort` panels.
        let mut edit_panel = wave_build_edit_panel(&node);
        edit_panel.set_visible(meta_bool(&node, WAVE_EDIT_OPEN_META));
        left.add_child(&edit_panel);
        let mut sort_panel = wave_build_sort_panel(&node);
        sort_panel.set_visible(meta_bool(&node, WAVE_SORT_OPEN_META));
        left.add_child(&sort_panel);
        // `cont.add(graph)`.
        if let Some(graph) = instantiate_wave_graph() {
            row.add_child(&graph);
        }
        self.rebuild(dialog);
    }

    /// `buildGroups`: sorts, filters and rebuilds every group card.
    fn rebuild(&mut self, dialog: &mut MindDialog) {
        let Some(mut list) = self.list.clone() else {
            return;
        };
        list.bind_mut().clear();
        let node = dialog.to_gd();
        wave_sort_groups(&node);
        let groups = meta_array(&node, WAVE_GROUPS_META);
        let search = meta_int(&node, WAVE_SEARCH_META, -1);
        let filter_type = meta_text(&node, WAVE_FILTER_META);
        let expanded = meta_int(&node, WAVE_EXPANDED_META, -1);
        let mut any = false;
        for index in 0..groups.len() {
            let Ok(group) = groups.at(index).try_to::<VarDictionary>() else {
                continue;
            };
            if search >= 0 && wave_spawned_at(&group, search) <= 0 {
                continue;
            }
            if !filter_type.is_empty() && dict_text(&group, "type") != filter_type {
                continue;
            }
            any = true;
            let card = build_wave_card(&node, index as i64, &group, expanded);
            let _ = list.bind_mut().add(&card).pad(2.0);
            list.bind_mut().row();
        }
        if groups.is_empty() {
            let label = factory::label(&kit::t("@waves.none").to_string());
            let _ = list.bind_mut().add(&label).pad(4.0);
            list.bind_mut().row();
        } else if !any {
            let label = factory::label(&kit::t("@none.found").to_string());
            let _ = list.bind_mut().add(&label).pad(4.0);
            list.bind_mut().row();
        }
        wave_refresh_graph(&node);
    }
}

/// `_build_edit_panel()`: copy / load / clear / reset.
fn wave_build_edit_panel(node: &Gd<MindDialog>) -> Gd<VBoxContainer> {
    // code-instantiated: the edit menu is a fixed four-action strip.
    let mut panel = VBoxContainer::new_alloc();
    panel.set_visible(false);
    let mut copy = factory::button(&kit::t("@copy.clipboard").to_string());
    let callable = Callable::from_fn("wave_copy", move |_args: &[&Variant]| {
        let json = editor_json("waves_json", &[]).unwrap_or_default();
        DisplayServer::singleton().clipboard_set(&json);
        ui_show_info(&kit::t("@waves.copied"));
    });
    copy.connect("pressed", &callable);
    panel.add_child(&copy);
    let mut load = factory::button(&kit::t("@load.clipboard").to_string());
    let target = node.clone();
    let callable = Callable::from_fn("wave_load", move |_args: &[&Variant]| {
        let text = DisplayServer::singleton().clipboard_get();
        if !as_bool(editor_call("set_waves_json", &[text.to_variant()])) {
            ui_show_info(&kit::t("@waves.invalid"));
            return;
        }
        let groups = wave_read_groups();
        let mut meta = target.clone();
        set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
        set_meta_bool(&mut meta, WAVE_EDIT_OPEN_META, false);
        request_refresh(&target, WAVE_REQUEST_BUILD);
    });
    load.connect("pressed", &callable);
    panel.add_child(&load);
    let mut clear = factory::button(&kit::t("@clear").to_string());
    let target = node.clone();
    let callable = Callable::from_fn("wave_clear", move |_args: &[&Variant]| {
        let mut meta = target.clone();
        set_meta_array(&mut meta, WAVE_GROUPS_META, &VarArray::new());
        wave_commit(&target);
        set_meta_bool(&mut meta, WAVE_EDIT_OPEN_META, false);
        request_refresh(&target, WAVE_REQUEST_BUILD);
    });
    clear.connect("pressed", &callable);
    panel.add_child(&clear);
    let mut reset = factory::button(&kit::t("@settings.reset").to_string());
    let target = node.clone();
    let callable = Callable::from_fn("wave_reset", move |_args: &[&Variant]| {
        let groups = editor_json("default_waves_json", &[])
            .map(|text| json_array(&text))
            .unwrap_or_default();
        let mut meta = target.clone();
        set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
        wave_commit(&target);
        set_meta_bool(&mut meta, WAVE_EDIT_OPEN_META, false);
        request_refresh(&target, WAVE_REQUEST_BUILD);
    });
    reset.connect("pressed", &callable);
    panel.add_child(&reset);
    panel
}

/// `_build_sort_panel()`: the sort keys + `@waves.sort.reverse`.
fn wave_build_sort_panel(node: &Gd<MindDialog>) -> Gd<VBoxContainer> {
    // code-instantiated: the sort panel is a fixed three-toggle strip.
    let mut panel = VBoxContainer::new_alloc();
    panel.set_visible(false);
    let mut sort_row = HBoxContainer::new_alloc();
    let current = meta_int(node, WAVE_SORT_META, 0);
    for (sort_value, name) in [(0i64, "begin"), (1, "health"), (2, "type")] {
        let mut button = factory::button(&kit::t(&format!("@waves.sort.{name}")).to_string());
        button.set_theme_type_variation("flatTogglet");
        button.set_toggle_mode(true);
        button.set_pressed(current == sort_value);
        let target = node.clone();
        let callable = Callable::from_fn("wave_sort_key", move |_args: &[&Variant]| {
            let mut meta = target.clone();
            set_meta_int(&mut meta, WAVE_SORT_META, sort_value);
            set_meta_bool(&mut meta, WAVE_SORT_OPEN_META, false);
            request_refresh(&target, WAVE_REQUEST_BUILD);
        });
        button.connect("pressed", &callable);
        sort_row.add_child(&button);
    }
    panel.add_child(&sort_row);
    let target = node.clone();
    let reverse = meta_bool(node, WAVE_REVERSE_META);
    let listener = Callable::from_fn("wave_sort_reverse", move |args: &[&Variant]| {
        let pressed = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        set_meta_bool(&mut target.clone(), WAVE_REVERSE_META, pressed);
        request_refresh(&target, WAVE_REQUEST_LIST);
    });
    panel.add_child(&mind_check(
        &kit::t("@waves.sort.reverse").to_string(),
        reverse,
        listener,
    ));
    panel
}

/// `SpawnGroup.getSpawned(wave)`: whether the group spawns on `wave`.
fn wave_spawned_at(group: &VarDictionary, wave: i64) -> i64 {
    let begin = dict_int(group, "begin");
    let end = match group.get("end") {
        Some(value) => as_int(Some(value)),
        None => WAVE_NEVER,
    };
    let spacing = dict_int(group, "spacing").max(1);
    if wave < begin || wave > end {
        return 0;
    }
    if (wave - begin) % spacing != 0 {
        return 0;
    }
    dict_int(group, "amount").max(1)
}

/// `groups.sort(Structs.comps(comparingFloat(sort.sort), comparingFloat(sort.secondary)))`.
fn wave_sort_groups(node: &Gd<MindDialog>) {
    let mut groups = meta_array(node, WAVE_GROUPS_META);
    if groups.is_empty() {
        return;
    }
    let sort = meta_int(node, WAVE_SORT_META, 0);
    let reverse = meta_bool(node, WAVE_REVERSE_META);
    let type_order = wave_unit_names();
    let health = wave_unit_health();
    let mut entries: Vec<Variant> = groups.iter_shared().collect();
    entries.sort_by(|first, second| {
        let first = first.try_to::<VarDictionary>().unwrap_or_default();
        let second = second.try_to::<VarDictionary>().unwrap_or_default();
        let key_a = wave_sort_key(&first, sort, &type_order, &health);
        let key_b = wave_sort_key(&second, sort, &type_order, &health);
        if key_a == key_b {
            return if sort == 0 {
                let a = wave_sort_key(&first, 2, &type_order, &health);
                let b = wave_sort_key(&second, 2, &type_order, &health);
                a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                dict_int(&first, "begin").cmp(&dict_int(&second, "begin"))
            };
        }
        key_a
            .partial_cmp(&key_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if reverse {
        entries.reverse();
    }
    for (index, value) in entries.iter().enumerate() {
        groups.set(index, value);
    }
    set_meta_array(&mut node.clone(), WAVE_GROUPS_META, &groups);
}

/// `_sort_key(group, sort)`.
fn wave_sort_key(
    group: &VarDictionary,
    sort: i64,
    type_order: &[String],
    health: &VarDictionary,
) -> f64 {
    match sort {
        1 => as_float(health.get(dict_text(group, "type").as_str())),
        2 => {
            let name = dict_text(group, "type");
            type_order
                .iter()
                .position(|entry| entry == &name)
                .map(|position| position as f64)
                .unwrap_or(-1.0)
        }
        _ => dict_int(group, "begin") as f64,
    }
}

/// One group card (`table.table(Tex.button, t -> ...)`).
fn build_wave_card(
    node: &Gd<MindDialog>,
    index: i64,
    group: &VarDictionary,
    expanded: i64,
) -> Gd<Control> {
    // code-instantiated: one card per spawn group; the count is data-driven.
    let mut card = PanelContainer::new_alloc();
    card.add_theme_stylebox_override("panel", &button_nine_patch("button"));
    let mut box_ = VBoxContainer::new_alloc();
    card.add_child(&box_);
    let mut header = Button::new_alloc();
    header.set_theme_type_variation("nonet");
    header.set_custom_minimum_size(Vector2::new(0.0, 34.0));
    let target = node.clone();
    let callable = Callable::from_fn("wave_expand", move |_args: &[&Variant]| {
        let mut meta = target.clone();
        let current = meta_int(&meta, WAVE_EXPANDED_META, -1);
        set_meta_int(
            &mut meta,
            WAVE_EXPANDED_META,
            if current == index { -1 } else { index },
        );
        request_refresh(&target, WAVE_REQUEST_LIST);
    });
    header.connect("pressed", &callable);
    box_.add_child(&header);
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_preset(LayoutPreset::FULL_RECT);
    row.set_mouse_filter(MouseFilter::IGNORE);
    row.add_theme_constant_override("separation", 0);
    header.add_child(&row);
    let unit_type = {
        let value = dict_text(group, "type");
        if value.is_empty() {
            String::from("?")
        } else {
            value
        }
    };
    // code-instantiated: the unit icon is resolved from the content atlas.
    let mut icon = TextureRect::new_alloc();
    if let Some(texture) = kit::icon_texture(&format!("unit-{unit_type}-ui")) {
        icon.set_texture(&texture);
    }
    icon.set_custom_minimum_size(Vector2::new(32.0, 32.0));
    icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
    icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    icon.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    icon.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&icon);
    let mut gap = Control::new_alloc();
    gap.set_custom_minimum_size(Vector2::new(3.0, 0.0));
    gap.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&gap);
    let mut name_label = factory::label(&wave_unit_name(&unit_type));
    name_label.add_theme_color_override("font_color", styles::ACCENT);
    let mut name_control = as_control(&name_label);
    name_control.set_custom_minimum_size(Vector2::new(110.0, 0.0));
    name_control.set_clip_contents(true);
    name_control.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    name_control.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&name_control);
    let mut spacer = Control::new_alloc();
    spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
    spacer.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&spacer);
    let begin = dict_int(group, "begin") + 1;
    let mut begin_label = factory::label(&begin.to_string());
    begin_label.add_theme_color_override("font_color", styles::ARC_LIGHT_GRAY);
    let mut begin_control = as_control(&begin_label);
    begin_control.set_custom_minimum_size(Vector2::new(45.0, 0.0));
    begin_control.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    begin_control.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&begin_control);
    let copy_target = node.clone();
    let copy = Callable::from_fn("wave_copy_group", move |_args: &[&Variant]| {
        wave_copy_group(&copy_target, index);
    });
    row.add_child(&wave_small_icon("copy", "@editor.copy", copy));
    let effect = dict_text(group, "effect");
    let effect_icon = if effect.is_empty() { "logic" } else { "warn" };
    let effect_tip = if effect.is_empty() {
        kit::t("@none").to_string()
    } else {
        effect.clone()
    };
    let effect_target = node.clone();
    let effect_action = Callable::from_fn("wave_effect", move |_args: &[&Variant]| {
        wave_show_effect_picker(&effect_target, index);
    });
    row.add_child(&wave_small_icon(effect_icon, &effect_tip, effect_action));
    let unit_target = node.clone();
    let unit_action = Callable::from_fn("wave_unit", move |_args: &[&Variant]| {
        wave_open_picker(
            &unit_target,
            "",
            &wave_unit_names(),
            "",
            &format!("unit:{index}"),
        );
    });
    row.add_child(&wave_small_icon("units", "@stat.unittype", unit_action));
    let remove_target = node.clone();
    let remove = Callable::from_fn("wave_remove", move |_args: &[&Variant]| {
        wave_remove_group(&remove_target, index);
    });
    row.add_child(&wave_small_icon("cancel", "@waves.remove", remove));
    if expanded == index {
        box_.add_child(&wave_group_fields(node, index, group));
    }
    card.upcast()
}

/// `group.type.localizedName` (`unit.<name>.name`).
fn wave_unit_name(unit_type: &str) -> String {
    let key = format!("@unit.{unit_type}.name");
    let value = kit::t(&key).to_string();
    if value == key || value.is_empty() {
        unit_type.to_string()
    } else {
        value
    }
}

/// `_small_icon(icon, tooltip, handler)`.
fn wave_small_icon(icon: &str, tooltip: &str, handler: Callable) -> Gd<Button> {
    let mut button = factory::icon_button(
        icon,
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    button.set_custom_minimum_size(Vector2::new(34.0, 34.0));
    button.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    for child in button.get_children().iter_shared() {
        if let Ok(row) = child.clone().try_cast::<HBoxContainer>() {
            for inner in row.get_children().iter_shared() {
                if let Ok(mut label) = inner.try_cast::<Label>() {
                    label.add_theme_font_size_override("font_size", 20);
                }
            }
        }
    }
    button.set_tooltip_text(&GString::from(tooltip));
    button.connect("pressed", &handler);
    button
}

/// `_group_fields(group, index)`: every `SpawnGroup` row.
fn wave_group_fields(node: &Gd<MindDialog>, index: i64, group: &VarDictionary) -> Gd<Control> {
    let mut fields = VBoxContainer::new_alloc();
    // `@waves.to` range + begin (`t.field("" + (group.begin + 1))`).
    let mut range_row = HBoxContainer::new_alloc();
    let begin = dict_int(group, "begin") + 1;
    let begin_target = node.clone();
    range_row.add_child(&wave_int_field(&begin.to_string(), 100.0, {
        Callable::from_fn("wave_begin", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            wave_set_field(&begin_target, index, "begin", (value - 1).to_variant());
        })
    }));
    range_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.to").to_string(),
    )));
    let end = match group.get("end") {
        Some(value) => as_int(Some(value)),
        None => WAVE_NEVER,
    };
    let end_text = if end == WAVE_NEVER {
        String::new()
    } else {
        (end + 1).to_string()
    };
    let end_target = node.clone();
    range_row.add_child(&wave_int_field(&end_text, 100.0, {
        Callable::from_fn("wave_end", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            wave_set_field(
                &end_target,
                index,
                "end",
                (if value > 0 { value - 1 } else { WAVE_NEVER }).to_variant(),
            );
        })
    }));
    fields.add_child(&range_row);
    // `@waves.every ... @waves.waves`.
    let mut spacing_row = HBoxContainer::new_alloc();
    spacing_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.every").to_string(),
    )));
    let spacing = match group.get("spacing") {
        Some(value) => as_int(Some(value)),
        None => 1,
    };
    let spacing_target = node.clone();
    spacing_row.add_child(&wave_int_field(&spacing.to_string(), 100.0, {
        Callable::from_fn("wave_spacing", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            if value > 0 {
                wave_set_field(&spacing_target, index, "spacing", value.to_variant());
            }
        })
    }));
    spacing_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.waves").to_string(),
    )));
    fields.add_child(&spacing_row);
    // amount + scaling (`1f / unitScaling` shown, Java's fixed(2)).
    let mut amount_row = HBoxContainer::new_alloc();
    let amount = match group.get("amount") {
        Some(value) => as_int(Some(value)),
        None => 1,
    };
    let amount_target = node.clone();
    amount_row.add_child(&wave_int_field(&amount.to_string(), 80.0, {
        Callable::from_fn("wave_amount", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            wave_set_field(&amount_target, index, "amount", value.to_variant());
        })
    }));
    let scaling = match group.get("scaling") {
        Some(value) => as_float(Some(value)),
        None => WAVE_NEVER as f64,
    };
    let per_spawn = if scaling <= 0.0 { 0.0 } else { 1.0 / scaling };
    amount_row.add_child(&as_control(&factory::label(" + ")));
    let scaling_target = node.clone();
    amount_row.add_child(&wave_float_field(&format!("{per_spawn:.2}"), 80.0, {
        Callable::from_fn("wave_scaling", move |args: &[&Variant]| {
            let value = as_float(args.first().map(|value| (*value).clone()));
            if value > 0.0 {
                wave_set_field(
                    &scaling_target,
                    index,
                    "scaling",
                    (1.0 / value).to_variant(),
                );
            }
        })
    }));
    amount_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.perspawn").to_string(),
    )));
    fields.add_child(&amount_row);
    // `@waves.max`.
    let mut max_row = HBoxContainer::new_alloc();
    let max = match group.get("max") {
        Some(value) => as_int(Some(value)),
        None => 40,
    };
    let max_target = node.clone();
    max_row.add_child(&wave_int_field(&max.to_string(), 80.0, {
        Callable::from_fn("wave_max", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            wave_set_field(&max_target, index, "max", value.to_variant());
        })
    }));
    max_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.max").to_string(),
    )));
    fields.add_child(&max_row);
    // `@waves.shields` + shieldScaling.
    let mut shield_row = HBoxContainer::new_alloc();
    let shields = match group.get("shields") {
        Some(value) => as_int(Some(value)),
        None => 0,
    };
    let shields_target = node.clone();
    shield_row.add_child(&wave_int_field(&shields.to_string(), 80.0, {
        Callable::from_fn("wave_shields", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            wave_set_field(&shields_target, index, "shields", value.to_variant());
        })
    }));
    shield_row.add_child(&as_control(&factory::label(" + ")));
    let shield_scaling = match group.get("shieldScaling") {
        Some(value) => as_int(Some(value)),
        None => 0,
    };
    let shield_scaling_target = node.clone();
    shield_row.add_child(&wave_int_field(&shield_scaling.to_string(), 80.0, {
        Callable::from_fn("wave_shield_scaling", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            wave_set_field(
                &shield_scaling_target,
                index,
                "shieldScaling",
                value.to_variant(),
            );
        })
    }));
    shield_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.shields").to_string(),
    )));
    fields.add_child(&shield_row);
    // `@waves.guardian` (`group.effect == StatusEffects.boss`).
    let boss = dict_text(group, "effect") == "boss";
    let guardian_target = node.clone();
    let guardian = Callable::from_fn("wave_guardian", move |args: &[&Variant]| {
        let pressed = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        if pressed {
            wave_set_field(
                &guardian_target,
                index,
                "effect",
                GString::from("boss").to_variant(),
            );
        } else {
            wave_erase_field(&guardian_target, index, "effect");
        }
        request_refresh(&guardian_target, WAVE_REQUEST_LIST);
    });
    fields.add_child(&mind_check(
        &kit::t("@waves.guardian").to_string(),
        boss,
        guardian,
    ));
    // `@waves.team` row (`MapObjectivesDialog.showTeamSelect`).
    let mut team_row = HBoxContainer::new_alloc();
    team_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.team").to_string(),
    )));
    let mut team_button = factory::button(&wave_team_label(group));
    let team_target = node.clone();
    let callable = Callable::from_fn("wave_team", move |_args: &[&Variant]| {
        wave_open_picker(
            &team_target,
            "",
            &palette_team_names(),
            "@settings.resetKey",
            &format!("team:{index}"),
        );
    });
    team_button.connect("pressed", &callable);
    team_row.add_child(&team_button);
    fields.add_child(&team_row);
    // `@waves.spawn` row (packed `Point2` position).
    let mut spawn_row = HBoxContainer::new_alloc();
    spawn_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.spawn").to_string(),
    )));
    let mut spawn_button = factory::button(&wave_spawn_label(group));
    let spawn_target = node.clone();
    let callable = Callable::from_fn("wave_spawn", move |_args: &[&Variant]| {
        wave_show_spawn_picker(&spawn_target, index);
    });
    spawn_button.connect("pressed", &callable);
    spawn_row.add_child(&spawn_button);
    fields.add_child(&spawn_row);
    fields.upcast()
}

/// `_int_field(value, width, setter)`: a field calling `setter(int(text))`.
fn wave_int_field(value: &str, width: f32, setter: Callable) -> Gd<LineEdit> {
    let mut field = text_field(value, 10);
    field.set_custom_minimum_size(Vector2::new(width, 0.0));
    let callable = Callable::from_fn("wave_int_field", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| value.to::<GString>().to_string())
            .unwrap_or_default();
        if let Some(number) = valid_int(&text) {
            let _ = setter.call(&[number.to_variant()]);
        }
    });
    field.connect("text_changed", &callable);
    field
}

/// `_float_field(value, width, setter)`.
fn wave_float_field(value: &str, width: f32, setter: Callable) -> Gd<LineEdit> {
    let mut field = text_field(value, 10);
    field.set_custom_minimum_size(Vector2::new(width, 0.0));
    let callable = Callable::from_fn("wave_float_field", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| value.to::<GString>().to_string())
            .unwrap_or_default();
        if let Some(number) = valid_float(&text) {
            let _ = setter.call(&[number.to_variant()]);
        }
    });
    field.connect("text_changed", &callable);
    field
}

/// `_commit()`: writes the working list back and refreshes the graph.
fn wave_commit(node: &Gd<MindDialog>) {
    let groups = meta_array(node, WAVE_GROUPS_META);
    let _ = editor_write("set_waves_json", &json_string(&groups.to_variant()));
    wave_refresh_graph(node);
}

/// Writes one field of one group, then commits.
fn wave_set_field(node: &Gd<MindDialog>, index: i64, key: &str, value: Variant) {
    let mut groups = meta_array(node, WAVE_GROUPS_META);
    if index < 0 || index >= groups.len() as i64 {
        return;
    }
    let Ok(mut group) = groups.at(index as usize).try_to::<VarDictionary>() else {
        return;
    };
    group.set(key, &value);
    groups.set(index as usize, &group.to_variant());
    set_meta_array(&mut node.clone(), WAVE_GROUPS_META, &groups);
    wave_commit(node);
}

/// Erases one group field (`group.erase("effect")`).
fn wave_erase_field(node: &Gd<MindDialog>, index: i64, key: &str) {
    let mut groups = meta_array(node, WAVE_GROUPS_META);
    if index < 0 || index >= groups.len() as i64 {
        return;
    }
    let Ok(mut group) = groups.at(index as usize).try_to::<VarDictionary>() else {
        return;
    };
    group.remove(key);
    groups.set(index as usize, &group.to_variant());
    set_meta_array(&mut node.clone(), WAVE_GROUPS_META, &groups);
    wave_commit(node);
}

/// `groups.insert(groups.indexOf(group) + 1, group.copy())`.
fn wave_copy_group(node: &Gd<MindDialog>, index: i64) {
    let mut groups = meta_array(node, WAVE_GROUPS_META);
    if index < 0 || index >= groups.len() as i64 {
        return;
    }
    let copy = groups
        .at(index as usize)
        .try_to::<VarDictionary>()
        .unwrap_or_default();
    groups.insert(index as usize + 1, &copy.duplicate_deep().to_variant());
    let mut meta = node.clone();
    set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
    set_meta_int(&mut meta, WAVE_EXPANDED_META, index + 1);
    wave_commit(node);
    request_refresh(node, WAVE_REQUEST_LIST);
}

fn wave_remove_group(node: &Gd<MindDialog>, index: i64) {
    let mut groups = meta_array(node, WAVE_GROUPS_META);
    if index < 0 || index >= groups.len() as i64 {
        return;
    }
    groups.remove(index as usize);
    let mut meta = node.clone();
    set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
    set_meta_int(&mut meta, WAVE_EXPANDED_META, -1);
    wave_commit(node);
    request_refresh(node, WAVE_REQUEST_LIST);
}

/// `showEffects(group)`: the status-effect picker.
fn wave_show_effect_picker(node: &Gd<MindDialog>, index: i64) {
    wave_open_picker(
        node,
        "",
        &wave_effect_names(),
        "@settings.resetKey",
        &format!("effect:{index}"),
    );
}

/// The spawn picker (`spawner.getSpawns()`: first 20 packed positions + all).
fn wave_show_spawn_picker(node: &Gd<MindDialog>, index: i64) {
    let mut names: Vec<String> = Vec::new();
    for value in editor_array("spawn_points_json").iter_shared() {
        if names.len() >= WAVE_SPAWN_CAP {
            break;
        }
        let packed = as_int(Some(value)) as i32;
        let (x, y) = unpack_point2(packed);
        names.push(format!("{x}, {y}"));
    }
    wave_open_picker(
        node,
        "@waves.spawn.select",
        &names,
        "@waves.spawn.all",
        &format!("spawn:{index}"),
    );
}

/// `_open_picker(title, items, reset_key, target)`.
fn wave_open_picker(
    node: &Gd<MindDialog>,
    title_key: &str,
    items: &[String],
    reset_key: &str,
    target: &str,
) {
    if kit::ui().is_none() {
        return;
    }
    set_meta_text(&mut node.clone(), WAVE_PICKER_META, target);
    wave_bind_picker(node);
    let mut list = VarArray::new();
    for item in items {
        list.push(&GString::from(item.as_str()).to_variant());
    }
    let mut context = VarDictionary::new();
    context.set("title", &GString::from(title_key).to_variant());
    context.set("items", &list.to_variant());
    context.set("reset", &GString::from(reset_key).to_variant());
    ui_open_dialog("list_select", &context);
}

/// `_connect_picker()`: the sibling `list_select` `selected` signal.
fn wave_bind_picker(node: &Gd<MindDialog>) {
    if node.has_meta(WAVE_PICKER_BOUND_META) {
        return;
    }
    let Some(mut picker) = dialog_node("list_select") else {
        return;
    };
    if !picker.has_signal("selected") {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("wave_picked", move |args: &[&Variant]| {
        let name = args
            .first()
            .map(|value| value.to::<GString>())
            .unwrap_or_default();
        set_meta_text(&mut target.clone(), WAVE_PICKED_META, &name.to_string());
        request_refresh_deferred(&target, WAVE_REQUEST_PICKED);
    });
    picker.connect("selected", &callable);
    node.clone()
        .set_meta(WAVE_PICKER_BOUND_META, &true.to_variant());
}

/// `_on_picked(picked_name)`: the pending picker target.
fn wave_apply_picked(node: &Gd<MindDialog>) {
    let target = meta_text(node, WAVE_PICKER_META);
    let name = meta_text(node, WAVE_PICKED_META);
    let mut meta = node.clone();
    set_meta_text(&mut meta, WAVE_PICKER_META, "");
    set_meta_text(&mut meta, WAVE_PICKED_META, "");
    let mut parts = target.splitn(2, ':');
    let kind = parts.next().unwrap_or_default();
    let index = parts.next().and_then(valid_int).unwrap_or(-1);
    match kind {
        "filter" => {
            set_meta_text(&mut node.clone(), WAVE_FILTER_META, &name);
        }
        "add" => {
            if !name.is_empty() {
                let mut groups = meta_array(node, WAVE_GROUPS_META);
                let mut group = VarDictionary::new();
                group.set("type", &GString::from(name.as_str()).to_variant());
                groups.push(&group);
                let mut meta = node.clone();
                set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
                set_meta_int(&mut meta, WAVE_EXPANDED_META, groups.len() as i64 - 1);
                wave_commit(node);
            }
        }
        "unit" => {
            if index >= 0 && !name.is_empty() {
                wave_set_field(
                    node,
                    index,
                    "type",
                    GString::from(name.as_str()).to_variant(),
                );
            }
        }
        "effect" => {
            if index >= 0 {
                if name.is_empty() {
                    wave_erase_field(node, index, "effect");
                } else {
                    wave_set_field(
                        node,
                        index,
                        "effect",
                        GString::from(name.as_str()).to_variant(),
                    );
                }
            }
        }
        "spawn" => {
            if index >= 0 {
                if name.is_empty() {
                    wave_set_field(node, index, "spawn", (-1i64).to_variant());
                } else {
                    let coords: Vec<&str> = name.split(',').collect();
                    if coords.len() == 2 {
                        let x = valid_int(coords[0].trim()).unwrap_or(0) as i32;
                        let y = valid_int(coords[1].trim()).unwrap_or(0) as i32;
                        wave_set_field(
                            node,
                            index,
                            "spawn",
                            i64::from(pack_point2(x, y)).to_variant(),
                        );
                    }
                }
            }
        }
        "team" if index >= 0 => {
            if name.is_empty() {
                wave_erase_field(node, index, "team");
            } else {
                let id = palette_team_names()
                    .iter()
                    .position(|team| team == &name)
                    .map(|position| position as i64)
                    .unwrap_or(-1);
                wave_set_field(node, index, "team", id.to_variant());
            }
        }
        _ => {}
    }
}

/// `_team_label(group)`.
fn wave_team_label(group: &VarDictionary) -> String {
    if group.get("team").is_none() {
        return kit::t("@none").to_string();
    }
    let id = dict_int(group, "team");
    palette_team_names()
        .get(id.max(0) as usize)
        .cloned()
        .unwrap_or_else(|| id.to_string())
}

/// `_spawn_label(group)`.
fn wave_spawn_label(group: &VarDictionary) -> String {
    let packed = match group.get("spawn") {
        Some(value) => as_int(Some(value)),
        None => -1,
    };
    if packed == -1 {
        return kit::t("@waves.spawn.all").to_string();
    }
    let (x, y) = unpack_point2(packed as i32);
    format!("{x}, {y}")
}

/// `_unit_names()` (`wave_units_json` rows).
fn wave_unit_rows() -> Vec<VarDictionary> {
    dict_rows(&editor_array("wave_units_json"))
}

fn wave_unit_names() -> Vec<String> {
    wave_unit_rows()
        .iter()
        .map(|row| dict_text(row, "name"))
        .collect()
}

/// `_unit_health()`.
fn wave_unit_health() -> VarDictionary {
    let mut health = VarDictionary::new();
    for row in wave_unit_rows() {
        health.set(
            &GString::from(dict_text(&row, "name").as_str()),
            &dict_float(&row, "health", 0.0).to_variant(),
        );
    }
    health
}

/// `_effect_names()` (`wave_effects_json`).
fn wave_effect_names() -> Vec<String> {
    editor_array("wave_effects_json")
        .iter_shared()
        .map(|value| variant_text(&value))
        .collect()
}

/// `_read_groups()`: the `spawns` tag, else the vanilla table.
fn wave_read_groups() -> VarArray {
    let parsed = editor_json("waves_json", &[])
        .map(|text| json_array(&text))
        .unwrap_or_default();
    if !parsed.is_empty() {
        return parsed;
    }
    // `state.rules.spawns.isEmpty() ? waves.get()`.
    editor_json("default_waves_json", &[])
        .map(|text| json_array(&text))
        .unwrap_or_default()
}

/// `graph.groups = groups; graph.rebuild()`.
fn wave_refresh_graph(node: &Gd<MindDialog>) {
    if let Some(mut graph) = find_named(node, WAVE_GRAPH_NODE)
        && graph.has_method("refresh")
    {
        let _ = graph.call("refresh", &[]);
    }
}

/// Instantiates `res://scenes/editor/wave_graph.tscn` (`graphScene`).
fn instantiate_wave_graph() -> Option<Gd<Control>> {
    let scene = godot::tools::try_load::<godot::classes::PackedScene>(&GString::from(
        "res://scenes/editor/wave_graph.tscn",
    ))
    .ok()?;
    let node = scene.instantiate()?;
    let mut control = node.try_cast::<Control>().ok()?;
    control.set_name(WAVE_GRAPH_NODE);
    control.set_h_size_flags(SizeFlags::EXPAND_FILL);
    control.set_v_size_flags(SizeFlags::EXPAND_FILL);
    control.set_custom_minimum_size(Vector2::new(240.0, 260.0));
    Some(control)
}

// ---------------------------------------------------------------------------
// MapObjectivesDialog (`map_objectives`, `MapObjectivesDialog.java`)
// ---------------------------------------------------------------------------

/// Panel width for the selected objective's fields (`PANEL_WIDTH`).
const OBJ_PANEL_WIDTH: f32 = 380.0;
/// Objective list (`MapObjectivesDialog.objectives`).
const OBJ_OBJECTIVES_META: &str = "mind_obj_objectives";
/// Selected objective index (`-1` for none).
const OBJ_SELECTED_META: &str = "mind_obj_selected";
/// The objective being edited in the class picker (`-1` = append).
const OBJ_PENDING_CLASS_META: &str = "mind_obj_pending_class";
/// The objective created by `@add` waiting for a canvas click (`canvas.query`).
const OBJ_PENDING_QUERY_META: &str = "mind_obj_pending_query";
/// Set while the `@clear` confirm is pending (`@settings.clear.confirm`).
const OBJ_PENDING_CLEAR_META: &str = "mind_obj_pending_clear";
/// The pending `list_select` target for content/team fields.
const OBJ_PICK_INDEX_META: &str = "mind_obj_pick_index";
const OBJ_PICK_FIELD_META: &str = "mind_obj_pick_field";
const OBJ_PICK_KIND_META: &str = "mind_obj_pick_kind";
const OBJ_PICKED_META: &str = "mind_obj_picked";
/// The color picker's answer target (`Color` interpreter).
const OBJ_COLOR_FIELD_META: &str = "mind_obj_color_field";
/// The `@LogicCode` answer target.
const OBJ_LOGIC_FIELD_META: &str = "mind_obj_logic_field";
const OBJ_LOGIC_BOUND_META: &str = "mind_obj_logic_bound";
/// Signal-binding guards.
const OBJ_PICKER_BOUND_META: &str = "mind_obj_picker_bound";
const OBJ_COLOR_BOUND_META: &str = "mind_obj_color_bound";
const OBJ_CONFIRM_BOUND_META: &str = "mind_obj_confirm_bound";
/// Node names for the code-built parts.
const OBJ_OVERLAY: &str = "ObjectivesOverlay";
const OBJ_CANVAS_NODE: &str = "ObjectivesCanvas";
const OBJ_PANEL_NODE: &str = "ObjectivesPanel";

const OBJ_REQUEST_PANEL: &str = "panel";
const OBJ_REQUEST_BUILD: &str = "build";
const OBJ_REQUEST_PICKED: &str = "picked";

/// `LStatement.aligns` / `nameToAlign` (`Arc Align` bit values).
const OBJ_ALIGN_NAMES: [(&str, i64); 9] = [
    ("topLeft", 10),
    ("top", 2),
    ("topRight", 18),
    ("left", 8),
    ("center", 1),
    ("right", 16),
    ("bottomLeft", 12),
    ("bottom", 4),
    ("bottomRight", 20),
];

/// The objectives editor: the canvas plus the selected objective's fields.
#[derive(Default)]
pub struct MapObjectivesDialog {
    /// `[{tag, fields: [{name, kind, flags}]}]`.
    classes: VarArray,
}

impl DialogController for MapObjectivesDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.objectives");
        dialog.should_pause = false;
        dialog.clear_buttons();
        let node = dialog.to_gd();
        // `buttons.button("@back", Icon.left, ...)`, `@add`, `@edit.menu`.
        dialog.add_close_button(153.0);
        let add = kit::t("@add").to_string();
        let add_target = node.clone();
        let callable = Callable::from_fn("objectives_add", move |_args: &[&Variant]| {
            set_meta_int(&mut add_target.clone(), OBJ_PENDING_CLASS_META, -1);
            objective_open_class_picker(&add_target);
        });
        dialog.add_button(&add, Some(callable), "add", 157.0);
        let edit = kit::t("@edit.menu").to_string();
        let edit_target = node.clone();
        let callable = Callable::from_fn("objectives_edit", move |_args: &[&Variant]| {
            objective_show_edit_menu(&edit_target);
        });
        dialog.add_button(&edit, Some(callable), "edit", 158.0);
        objective_bind_confirm(&node);
        objective_bind_picker(&node);
        objective_bind_color(&node);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let request = take_request(dialog);
        match request.as_str() {
            OBJ_REQUEST_PANEL => self.rebuild_panel(dialog),
            OBJ_REQUEST_PICKED => {
                objective_apply_picked(&dialog.to_gd());
                self.rebuild_panel(dialog);
            }
            OBJ_REQUEST_BUILD => self.build(dialog),
            // `shown()`: read the descriptors + objectives, then build.
            _ => self.setup(dialog),
        }
    }
}

impl MapObjectivesDialog {
    fn setup(&mut self, dialog: &mut MindDialog) {
        let node = dialog.to_gd();
        self.classes = editor_array("objective_classes_json");
        let marker_classes = editor_array("marker_classes_json");
        let objectives = editor_array("objectives_json");
        let mut meta = node.clone();
        set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &objectives);
        set_meta_int(&mut meta, OBJ_SELECTED_META, -1);
        set_meta_int(&mut meta, OBJ_PENDING_CLASS_META, -1);
        set_meta_dict(&mut meta, OBJ_PENDING_QUERY_META, &VarDictionary::new());
        set_meta_bool(&mut meta, OBJ_PENDING_CLEAR_META, false);
        objective_normalize_positions(&node);
        let _ = marker_classes;
        self.build(dialog);
    }

    fn build(&mut self, dialog: &mut MindDialog) {
        if dialog.content().is_none() {
            return;
        }
        dialog.clear_content();
        let mut root = dialog.content_table();
        let node = dialog.to_gd();
        let mut row = HBoxContainer::new_alloc();
        row.set_h_size_flags(SizeFlags::EXPAND_FILL);
        row.set_v_size_flags(SizeFlags::EXPAND_FILL);
        row.add_theme_constant_override("separation", 10);
        root.bind_mut().add(&row).grow_x().grow_y().pad(4.0);
        // `canvas = new MapObjectivesCanvas()` (the Java dialog is canvas-first).
        if let Some(canvas) = instantiate_objectives_canvas() {
            row.add_child(&canvas);
            objective_bind_canvas(&node, &canvas);
        }
        // code-instantiated: the field panel is driven by the class descriptors.
        let mut scroll = MindScroll::new_alloc();
        scroll.bind_mut().set_scrolling_disabled(true, false);
        let mut scroll_control = as_control(&scroll);
        scroll_control.set_custom_minimum_size(Vector2::new(OBJ_PANEL_WIDTH, 0.0));
        scroll_control.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut panel = scroll.bind_mut().content();
        panel.set_name(OBJ_PANEL_NODE);
        row.add_child(&scroll_control);
        let _ = &mut panel;
        self.rebuild_panel(dialog);
    }

    /// The selected objective's editor (type, position actions, field rows).
    fn rebuild_panel(&mut self, dialog: &mut MindDialog) {
        let node = dialog.to_gd();
        let Some(mut panel) = find_named(&node, OBJ_PANEL_NODE)
            .and_then(|child| child.try_cast::<VBoxContainer>().ok())
        else {
            return;
        };
        for child in panel.get_children().iter_shared() {
            let mut child = child.clone();
            panel.remove_child(&child);
            child.queue_free();
        }
        let objectives = meta_array(&node, OBJ_OBJECTIVES_META);
        let selected = meta_int(&node, OBJ_SELECTED_META, -1);
        // With none selected there is no side panel at all.
        panel.set_visible(selected >= 0 && selected < objectives.len() as i64);
        if !panel.is_visible() {
            return;
        }
        let Ok(objective) = objectives.at(selected as usize).try_to::<VarDictionary>() else {
            return;
        };
        let tag = dict_text(&objective, "class");
        let mut header = HBoxContainer::new_alloc();
        header.add_theme_constant_override("separation", 4);
        let mut type_button = factory::button(&objective_type_name(&tag));
        type_button.set_theme_type_variation("flatt");
        type_button.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let type_target = node.clone();
        let callable = Callable::from_fn("objective_type", move |_args: &[&Variant]| {
            let selected = meta_int(&type_target, OBJ_SELECTED_META, -1);
            set_meta_int(&mut type_target.clone(), OBJ_PENDING_CLASS_META, selected);
            objective_open_class_picker(&type_target);
        });
        type_button.connect("pressed", &callable);
        header.add_child(&type_button);
        let up_target = node.clone();
        let callable = Callable::from_fn("objective_up", move |_args: &[&Variant]| {
            objective_move(&up_target, -1);
        });
        header.add_child(&objective_small_button("up", callable));
        let down_target = node.clone();
        let callable = Callable::from_fn("objective_down", move |_args: &[&Variant]| {
            objective_move(&down_target, 1);
        });
        header.add_child(&objective_small_button("down", callable));
        let remove_target = node.clone();
        let callable = Callable::from_fn("objective_remove", move |_args: &[&Variant]| {
            objective_remove_selected(&remove_target);
        });
        header.add_child(&objective_small_button("cancel", callable));
        panel.add_child(&header);
        for field in dict_array(&self.class_entry(&tag), "fields").iter_shared() {
            let Ok(field) = field.try_to::<VarDictionary>() else {
                continue;
            };
            if !objective_visible_field(&field) {
                continue;
            }
            let field_name = dict_text(&field, "name");
            if field_name == "class" || field_name == "editorPos" || field_name == "parents" {
                continue;
            }
            let Some(editor) = objective_field_row(&node, selected, &field, &objective) else {
                continue;
            };
            panel.add_child(&editor);
        }
    }

    fn class_entry(&self, tag: &str) -> VarDictionary {
        for entry in self.classes.iter_shared() {
            let Ok(entry) = entry.try_to::<VarDictionary>() else {
                continue;
            };
            if dict_text(&entry, "tag") == tag {
                return entry;
            }
        }
        VarDictionary::new()
    }
}

/// `_objective_type_name(tag)`.
fn objective_type_name(tag: &str) -> String {
    kit::t(&format!("@objective.{}.name", tag.to_lowercase())).to_string()
}

/// `ObjectiveMarker.typeName()`: `marker.<class>.name`.
fn objective_marker_name(tag: &str) -> String {
    kit::t(&format!("@marker.{}.name", tag.to_lowercase())).to_string()
}

/// A 34 px icon button (`_small_button`).
fn objective_small_button(icon: &str, handler: Callable) -> Gd<Button> {
    let mut button = factory::icon_button(
        icon,
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    button.set_custom_minimum_size(Vector2::new(34.0, 34.0));
    button.connect("pressed", &handler);
    button
}

/// `_is_visible_field(field)` (`FieldFlags`).
fn objective_visible_field(field: &VarDictionary) -> bool {
    let flags = dict_array(field, "flags");
    !flags_has(&flags, "hidden") && !flags_has(&flags, "synthetic")
}

fn flags_has(flags: &VarArray, name: &str) -> bool {
    flags.iter_shared().any(|value| {
        value
            .try_to::<GString>()
            .map(|value| value.to_string())
            .ok()
            .as_deref()
            == Some(name)
    })
}

/// The objective dictionary at `index` (`{}` when absent).
fn objective_at(node: &Gd<MindDialog>, index: i64) -> VarDictionary {
    let objectives = meta_array(node, OBJ_OBJECTIVES_META);
    objectives
        .get(index.max(0) as usize)
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// `_default_of(field)`.
fn objective_default_of(field: &VarDictionary) -> Variant {
    let kind = dict_text(field, "kind");
    if kind.starts_with("content:") {
        return GString::from("copper").to_variant();
    }
    match kind.as_str() {
        "team" => 1i64.to_variant(),
        "indexbool" => 1i64.to_variant(),
        "labelflag" => 3i64.to_variant(),
        "alignment" => 1i64.to_variant(),
        "texture" => {
            let mut value = VarDictionary::new();
            value.set("string", &GString::from("white").to_variant());
            value.to_variant()
        }
        "color" => GString::from("ffd37fff").to_variant(),
        "vec2i" => {
            let mut value = VarDictionary::new();
            value.set("x", &0i64.to_variant());
            value.set("y", &0i64.to_variant());
            value.to_variant()
        }
        "vec2f" => {
            let mut value = VarDictionary::new();
            value.set("x", &0.0f64.to_variant());
            value.set("y", &0.0f64.to_variant());
            value.to_variant()
        }
        "marker" => VarDictionary::new().to_variant(),
        "verts" => VarArray::new().to_variant(),
        _ => {
            if kind.starts_with("seq:") || kind.starts_with("map:") {
                return VarArray::new().to_variant();
            }
            Variant::nil()
        }
    }
}

/// Sets one field on one objective and writes the objectives tag.
fn objective_set_field(node: &Gd<MindDialog>, index: i64, field_name: &str, value: Variant) {
    let mut objectives = meta_array(node, OBJ_OBJECTIVES_META);
    if index < 0 || index >= objectives.len() as i64 {
        return;
    }
    let Ok(mut objective) = objectives.at(index as usize).try_to::<VarDictionary>() else {
        return;
    };
    objective.set(field_name, &value);
    objectives.set(index as usize, &objective.to_variant());
    set_meta_array(&mut node.clone(), OBJ_OBJECTIVES_META, &objectives);
    objective_commit(node);
}

/// `_commit()`: writes the objectives JSON.
fn objective_commit(node: &Gd<MindDialog>) {
    let objectives = meta_array(node, OBJ_OBJECTIVES_META);
    let _ = editor_write(
        "set_objectives_json",
        &json_string(&objectives.to_variant()),
    );
}

/// `rebuildObjectives`: programmatic objectives (editorX == -999) get a grid
/// layout before the canvas draws them.
fn objective_normalize_positions(node: &Gd<MindDialog>) {
    let mut objectives = meta_array(node, OBJ_OBJECTIVES_META);
    let default_pos = pack_point2(-999, -999);
    let unplaced = objectives.iter_shared().any(|value| {
        value
            .try_to::<VarDictionary>()
            .map(|objective| match objective.get("editorPos") {
                Some(position) => as_int(Some(position)) as i32 == default_pos,
                None => true,
            })
            .unwrap_or(true)
    });
    if !unplaced {
        return;
    }
    let width: i64 = 7;
    let total_width = objectives.len() as i64 * width;
    let mut columns = objectives.len() as i64;
    let mut rows = 1i64;
    if total_width > 100 {
        rows = total_width / 100;
        columns = 100 / width;
    }
    let mut index = 0usize;
    for y in 0..rows {
        for x in 0..columns {
            if index >= objectives.len() {
                break;
            }
            let Ok(mut objective) = objectives.at(index).try_to::<VarDictionary>() else {
                index += 1;
                continue;
            };
            objective.set(
                "editorPos",
                &i64::from(pack_point2((x * width) as i32, y as i32)).to_variant(),
            );
            objectives.set(index, &objective.to_variant());
            index += 1;
        }
    }
    if index > 0 {
        set_meta_array(&mut node.clone(), OBJ_OBJECTIVES_META, &objectives);
        objective_commit(node);
    }
}

/// `_move(direction)`.
fn objective_move(node: &Gd<MindDialog>, direction: i64) {
    let selected = meta_int(node, OBJ_SELECTED_META, -1);
    let target = selected + direction;
    let mut objectives = meta_array(node, OBJ_OBJECTIVES_META);
    if selected < 0 || target < 0 || target >= objectives.len() as i64 {
        return;
    }
    let first = objectives.at(selected as usize);
    let second = objectives.at(target as usize);
    objectives.set(selected as usize, &second);
    objectives.set(target as usize, &first);
    let mut meta = node.clone();
    set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &objectives);
    set_meta_int(&mut meta, OBJ_SELECTED_META, target);
    objective_commit(node);
    request_refresh(node, OBJ_REQUEST_BUILD);
}

/// `_remove_selected()`.
fn objective_remove_selected(node: &Gd<MindDialog>) {
    let selected = meta_int(node, OBJ_SELECTED_META, -1);
    let mut objectives = meta_array(node, OBJ_OBJECTIVES_META);
    if selected < 0 || selected >= objectives.len() as i64 {
        return;
    }
    objectives.remove(selected as usize);
    let mut meta = node.clone();
    set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &objectives);
    set_meta_int(&mut meta, OBJ_SELECTED_META, -1);
    objective_commit(node);
    request_refresh(node, OBJ_REQUEST_BUILD);
}

/// The `@add` objective type picker (`getProvider(MapObjective.class)`).
fn objective_open_class_picker(node: &Gd<MindDialog>) {
    let classes = editor_array("objective_classes_json");
    if classes.is_empty() {
        return;
    }
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    // code-instantiated: per-action popup panels have no static scene.
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(3);
    grid.add_theme_constant_override("h_separation", 4);
    grid.add_theme_constant_override("v_separation", 4);
    for row in dict_rows(&classes) {
        let tag = dict_text(&row, "tag");
        let mut button = factory::button(&objective_type_name(&tag));
        button.set_theme_type_variation("flatt");
        button.set_text_alignment(HorizontalAlignment::LEFT);
        button.set_custom_minimum_size(Vector2::new(195.0, 56.0));
        let target = node.clone();
        let callable = Callable::from_fn("objective_pick_class", move |_args: &[&Variant]| {
            objective_pick_class(&target, &tag);
        });
        button.connect("pressed", &callable);
        grid.add_child(&button);
    }
    let title = kit::t("@add").to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        OBJ_OVERLAY,
        &title,
        640.0,
        380.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.8),
    ) {
        overlay.add_content(&grid);
    }
}

/// `_pick_class(tag, overlay)` then hide: append or replace the objective.
fn objective_pick_class(node: &Gd<MindDialog>, tag: &str) {
    free_named(node, OBJ_OVERLAY);
    let created = editor_json("objective_default_json", &[GString::from(tag).to_variant()])
        .map(|text| json_object(&text))
        .unwrap_or_default();
    if created.is_empty() {
        return;
    }
    let pending = meta_int(node, OBJ_PENDING_CLASS_META, -1);
    let mut objectives = meta_array(node, OBJ_OBJECTIVES_META);
    if pending >= 0 && pending < objectives.len() as i64 {
        // Keep the canvas position when changing class.
        let old = objectives
            .at(pending as usize)
            .try_to::<VarDictionary>()
            .unwrap_or_default();
        let mut created = created;
        if let Some(position) = old.get("editorPos") {
            created.set("editorPos", &position);
        }
        objectives.set(pending as usize, &created.to_variant());
        let mut meta = node.clone();
        set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &objectives);
        set_meta_int(&mut meta, OBJ_SELECTED_META, pending);
        set_meta_int(&mut meta, OBJ_PENDING_CLASS_META, -1);
        objective_commit(node);
        request_refresh(node, OBJ_REQUEST_BUILD);
        return;
    }
    set_meta_int(&mut node.clone(), OBJ_PENDING_CLASS_META, -1);
    // `@add` -> `canvas::query`: click the canvas to place it (`placeQuery`).
    set_meta_dict(&mut node.clone(), OBJ_PENDING_QUERY_META, &created);
    if let Some(mut canvas) = find_named(node, OBJ_CANVAS_NODE)
        && canvas.has_method("set_querying")
    {
        let _ = canvas.call("set_querying", &[true.to_variant()]);
    }
    ui_show_info(&kit::t("@editor.objectives"));
}

/// `@edit.menu`: copy/load/clear (`:496-522`).
fn objective_show_edit_menu(node: &Gd<MindDialog>) {
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    let mut rows = VBoxContainer::new_alloc();
    let objectives = meta_array(node, OBJ_OBJECTIVES_META);
    let mut copy = factory::icon_button(
        "copy",
        &kit::t("@copy.clipboard").to_string(),
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    copy.set_disabled(objectives.is_empty());
    let target = node.clone();
    let callable = Callable::from_fn("objective_copy", move |_args: &[&Variant]| {
        let objectives = meta_array(&target, OBJ_OBJECTIVES_META);
        DisplayServer::singleton().clipboard_set(&json_string(&objectives.to_variant()));
        ui_show_info(&kit::t("@copied"));
        free_named(&target, OBJ_OVERLAY);
    });
    copy.connect("pressed", &callable);
    rows.add_child(&copy);
    let mut load = factory::icon_button(
        "download",
        &kit::t("@load.clipboard").to_string(),
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let target = node.clone();
    let callable = Callable::from_fn("objective_load", move |_args: &[&Variant]| {
        let text = DisplayServer::singleton().clipboard_get();
        let parsed = json_value(&text);
        let Ok(objectives) = parsed.try_to::<VarArray>() else {
            ui_show_info(&kit::t("@waves.invalid"));
            free_named(&target, OBJ_OVERLAY);
            return;
        };
        let mut meta = target.clone();
        set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &objectives);
        set_meta_int(&mut meta, OBJ_SELECTED_META, -1);
        objective_commit(&target);
        free_named(&target, OBJ_OVERLAY);
        request_refresh(&target, OBJ_REQUEST_BUILD);
    });
    load.connect("pressed", &callable);
    rows.add_child(&load);
    let mut clear = factory::icon_button(
        "cancel",
        &kit::t("@clear").to_string(),
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let target = node.clone();
    let callable = Callable::from_fn("objective_clear", move |_args: &[&Variant]| {
        set_meta_bool(&mut target.clone(), OBJ_PENDING_CLEAR_META, true);
        free_named(&target, OBJ_OVERLAY);
        ui_show_confirm(&format!(
            "{}\n{}",
            kit::t("@confirm"),
            kit::t("@settings.clear.confirm")
        ));
    });
    clear.connect("pressed", &callable);
    rows.add_child(&clear);
    let mut back = factory::button(&kit::t("@back").to_string());
    let target = node.clone();
    let callable = Callable::from_fn("objective_back", move |_args: &[&Variant]| {
        free_named(&target, OBJ_OVERLAY);
    });
    back.connect("pressed", &callable);
    rows.add_child(&back);
    let title = kit::t("@edit.menu").to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        OBJ_OVERLAY,
        &title,
        320.0,
        320.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.8),
    ) {
        overlay.add_content(&rows);
    }
}

/// `_connect_confirm()`: the `@clear` answer.
fn objective_bind_confirm(node: &Gd<MindDialog>) {
    if node.has_meta(OBJ_CONFIRM_BOUND_META) {
        return;
    }
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_signal("confirm_result") {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("objective_confirm", move |args: &[&Variant]| {
        let confirmed = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        let mut meta = target.clone();
        if !meta_bool(&meta, OBJ_PENDING_CLEAR_META) {
            return;
        }
        set_meta_bool(&mut meta, OBJ_PENDING_CLEAR_META, false);
        if !confirmed {
            return;
        }
        set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &VarArray::new());
        set_meta_int(&mut meta, OBJ_SELECTED_META, -1);
        objective_commit(&target);
        request_refresh_deferred(&target, OBJ_REQUEST_BUILD);
    });
    ui.connect("confirm_result", &callable);
    node.clone()
        .set_meta(OBJ_CONFIRM_BOUND_META, &true.to_variant());
}

/// One byte of a `to_rgba32`-style hex string.
fn hex_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The canvas (`MapObjectivesCanvas`) signal wiring (one bind per instance).
fn objective_bind_canvas(node: &Gd<MindDialog>, canvas: &Gd<Control>) {
    let mut canvas = canvas.clone();
    if canvas.has_signal("node_selected") {
        let target = node.clone();
        let callable = Callable::from_fn("objective_selected", move |args: &[&Variant]| {
            let index = as_int(args.first().map(|value| (*value).clone()));
            set_meta_int(&mut target.clone(), OBJ_SELECTED_META, index);
            request_refresh(&target, OBJ_REQUEST_PANEL);
        });
        canvas.connect("node_selected", &callable);
    }
    if canvas.has_signal("node_moved") {
        let target = node.clone();
        let callable = Callable::from_fn("objective_moved", move |args: &[&Variant]| {
            let index = as_int(args.first().map(|value| (*value).clone()));
            let x = as_int(args.get(1).map(|value| (*value).clone())) as i32;
            let y = as_int(args.get(2).map(|value| (*value).clone())) as i32;
            objective_set_field(
                &target,
                index,
                "editorPos",
                i64::from(pack_point2(x, y)).to_variant(),
            );
        });
        canvas.connect("node_moved", &callable);
    }
    if canvas.has_signal("query_placed") {
        let target = node.clone();
        let callable = Callable::from_fn("objective_placed", move |args: &[&Variant]| {
            let x = as_int(args.first().map(|value| (*value).clone())) as i32;
            let y = as_int(args.get(1).map(|value| (*value).clone())) as i32;
            let mut pending = meta_dict(&target, OBJ_PENDING_QUERY_META);
            pending.set("editorPos", &i64::from(pack_point2(x, y)).to_variant());
            let mut objectives = meta_array(&target, OBJ_OBJECTIVES_META);
            objectives.push(&pending);
            let mut meta = target.clone();
            set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &objectives);
            set_meta_dict(&mut meta, OBJ_PENDING_QUERY_META, &VarDictionary::new());
            set_meta_int(&mut meta, OBJ_SELECTED_META, objectives.len() as i64 - 1);
            if let Some(mut canvas) = find_named(&target, OBJ_CANVAS_NODE)
                && canvas.has_method("set_querying")
            {
                let _ = canvas.call("set_querying", &[false.to_variant()]);
            }
            objective_commit(&target);
            request_refresh(&target, OBJ_REQUEST_BUILD);
        });
        canvas.connect("query_placed", &callable);
    }
}

/// Instantiates `res://scenes/editor/map_objectives_canvas.tscn`.
fn instantiate_objectives_canvas() -> Option<Gd<Control>> {
    let scene = godot::tools::try_load::<godot::classes::PackedScene>(&GString::from(
        "res://scenes/editor/map_objectives_canvas.tscn",
    ))
    .ok()?;
    let node = scene.instantiate()?;
    let mut control = node.try_cast::<Control>().ok()?;
    control.set_name(OBJ_CANVAS_NODE);
    control.set_h_size_flags(SizeFlags::EXPAND_FILL);
    control.set_v_size_flags(SizeFlags::EXPAND_FILL);
    control.set_custom_minimum_size(Vector2::new(480.0, 320.0));
    Some(control)
}

/// `_connect_picker()` + `_open_list_picker`: the `list_select` flow.
fn objective_bind_picker(node: &Gd<MindDialog>) {
    if node.has_meta(OBJ_PICKER_BOUND_META) {
        return;
    }
    let Some(mut picker) = dialog_node("list_select") else {
        return;
    };
    if !picker.has_signal("selected") {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("objective_picked", move |args: &[&Variant]| {
        let name = args
            .first()
            .map(|value| value.to::<GString>())
            .unwrap_or_default();
        set_meta_text(&mut target.clone(), OBJ_PICKED_META, &name.to_string());
        request_refresh_deferred(&target, OBJ_REQUEST_PICKED);
    });
    picker.connect("selected", &callable);
    node.clone()
        .set_meta(OBJ_PICKER_BOUND_META, &true.to_variant());
}

/// `_open_content_picker(content_type_name, setter)` via `list_select`.
fn objective_open_content_picker(node: &Gd<MindDialog>, index: i64, field_name: &str, kind: &str) {
    let Some(mut host) = kit::sim_host() else {
        return;
    };
    if !host.has_method("content_list") {
        return;
    }
    let listed = host.call("content_list", &[GString::from(kind).to_variant()]);
    let Ok(items) = listed.try_to::<PackedStringArray>() else {
        return;
    };
    if items.is_empty() {
        return;
    }
    let names: Vec<String> = items
        .as_slice()
        .iter()
        .map(|value| value.to_string())
        .collect();
    let mut meta = node.clone();
    set_meta_int(&mut meta, OBJ_PICK_INDEX_META, index);
    set_meta_text(&mut meta, OBJ_PICK_FIELD_META, field_name);
    set_meta_text(&mut meta, OBJ_PICK_KIND_META, "content");
    objective_open_list_picker(node, "", &names);
}

/// `_open_team_picker(setter)`: the base-team picker (`palette_teams`).
fn objective_open_team_picker(node: &Gd<MindDialog>, index: i64, field_name: &str) {
    let names = palette_team_names();
    let mut meta = node.clone();
    set_meta_int(&mut meta, OBJ_PICK_INDEX_META, index);
    set_meta_text(&mut meta, OBJ_PICK_FIELD_META, field_name);
    set_meta_text(&mut meta, OBJ_PICK_KIND_META, "team");
    objective_open_list_picker(node, "", &names);
}

/// `_open_list_picker(items, on_selected)`.
fn objective_open_list_picker(node: &Gd<MindDialog>, title_key: &str, items: &[String]) {
    if kit::ui().is_none() {
        return;
    }
    objective_bind_picker(node);
    let mut list = VarArray::new();
    for item in items {
        list.push(&GString::from(item.as_str()).to_variant());
    }
    let mut context = VarDictionary::new();
    context.set("title", &GString::from(title_key).to_variant());
    context.set("items", &list.to_variant());
    context.set("reset", &GString::from("").to_variant());
    ui_open_dialog("list_select", &context);
}

/// `_on_picked(selected_name)`: answers the pending content/team field.
fn objective_apply_picked(node: &Gd<MindDialog>) {
    let name = meta_text(node, OBJ_PICKED_META);
    set_meta_text(&mut node.clone(), OBJ_PICKED_META, "");
    if name.is_empty() {
        return;
    }
    let index = meta_int(node, OBJ_PICK_INDEX_META, -1);
    let field_name = meta_text(node, OBJ_PICK_FIELD_META);
    let kind = meta_text(node, OBJ_PICK_KIND_META);
    let value = if kind == "team" {
        team_id_by_name(&name).to_variant()
    } else {
        GString::from(name.as_str()).to_variant()
    };
    objective_set_field(node, index, &field_name, value);
}

/// The `Color` interpreter's picker binding.
fn objective_bind_color(node: &Gd<MindDialog>) {
    if node.has_meta(OBJ_COLOR_BOUND_META) {
        return;
    }
    let Some(mut picker) = dialog_node("picker") else {
        return;
    };
    if !picker.has_signal("color_selected") {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("objective_color", move |args: &[&Variant]| {
        let Some(value) = args.first() else {
            return;
        };
        let Ok(color) = value.try_to::<Color>() else {
            return;
        };
        let index = meta_int(&target, OBJ_SELECTED_META, -1);
        let field = meta_text(&target, OBJ_COLOR_FIELD_META);
        if index < 0 || field.is_empty() {
            return;
        }
        let hex = format!(
            "{:02x}{:02x}{:02x}{:02x}",
            hex_byte(color.r),
            hex_byte(color.g),
            hex_byte(color.b),
            hex_byte(color.a)
        );
        objective_set_field(
            &target,
            index,
            &field,
            GString::from(hex.as_str()).to_variant(),
        );
        request_refresh(&target, OBJ_REQUEST_PANEL);
    });
    picker.connect("color_selected", &callable);
    node.clone()
        .set_meta(OBJ_COLOR_BOUND_META, &true.to_variant());
}

/// `_open_color_picker(current, setter)`.
fn objective_open_color_picker(node: &Gd<MindDialog>, current: &str, field_name: &str) {
    if kit::ui().is_none() {
        return;
    }
    if dialog_node("picker").is_none() {
        return;
    }
    set_meta_text(&mut node.clone(), OBJ_COLOR_FIELD_META, field_name);
    objective_bind_color(node);
    // `picker.set_color(...)` has no native equivalent: the seed colour rides
    // the `open_dialog` context (`{"color": "#rrggbbaa", "alpha": true}`), the
    // same seam `dialogs/batch_g.rs` documents for `ColorPicker.show`.
    let seed = if current.len() == 8 {
        current.to_string()
    } else {
        String::from("ffd37f")
    };
    let mut context = VarDictionary::new();
    context.set("color", &GString::from(seed.as_str()).to_variant());
    context.set("alpha", &true.to_variant());
    ui_open_dialog("picker", &context);
}

/// `_color_from_hex(hex)`.
fn color_from_hex(hex: &str) -> Color {
    if hex.len() == 8 {
        let parse = |range: std::ops::Range<usize>| {
            hex.get(range)
                .and_then(|part| u8::from_str_radix(part, 16).ok())
                .unwrap_or(0)
        };
        return Color::from_rgba8(parse(0..2), parse(2..4), parse(4..6), parse(6..8));
    }
    styles::ACCENT
}

/// `_field_row(objective, tag, field)`: label + the type interpreter.
fn objective_field_row(
    node: &Gd<MindDialog>,
    index: i64,
    field: &VarDictionary,
    objective: &VarDictionary,
) -> Option<Gd<Control>> {
    let field_name = dict_text(field, "name");
    if field_name == "class" || field_name == "editorPos" {
        return None;
    }
    if field_name == "parents" || flags_has(&dict_array(field, "flags"), "synthetic") {
        return None;
    }
    let kind = dict_text(field, "kind");
    let flags = dict_array(field, "flags");
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_child(&as_control(&factory::label(&format!("{field_name}:"))));
    if let Some(inner) = kind.strip_prefix("seq:") {
        box_.add_child(&objective_seq_editor(
            node,
            index,
            &field_name,
            inner,
            &flags,
        ));
        return Some(box_.upcast());
    }
    let value = objective
        .get(&GString::from(field_name.as_str()))
        .unwrap_or_else(|| objective_default_of(field));
    let value_sink = objective_field_value(node, index, &field_name);
    box_.add_child(&objective_editor(
        node,
        index,
        &field_name,
        &value_sink,
        &value,
        &kind,
        &flags,
        false,
    ));
    Some(box_.upcast())
}

/// One editable value: Java's `FieldInterpreter.get`/`set` pair. The same
/// interpreter renders the objective field itself, a `Seq` element and an
/// `ObjectiveMarker` sub-field (GAPS #46 removed the raw-text fallbacks).
#[derive(Clone)]
struct ObjectiveValue {
    get: Rc<dyn Fn() -> Variant>,
    set: Rc<dyn Fn(Variant)>,
}

impl ObjectiveValue {
    fn new(get: impl Fn() -> Variant + 'static, set: impl Fn(Variant) + 'static) -> Self {
        Self {
            get: Rc::new(get),
            set: Rc::new(set),
        }
    }

    fn get(&self) -> Variant {
        (self.get)()
    }

    fn set(&self, value: Variant) {
        (self.set)(value);
    }

    /// One dictionary key of this value (`ObjectiveMarker` sub-fields,
    /// `TextureHolder.string`, `Vec2` axes).
    fn field(&self, key: &str) -> Self {
        let getter = self.clone();
        let setter = self.clone();
        let key_get = String::from(key);
        let key_set = String::from(key);
        Self::new(
            move || {
                getter
                    .get()
                    .try_to::<VarDictionary>()
                    .ok()
                    .and_then(|dict| dict.get(&GString::from(key_get.as_str())))
                    .unwrap_or(Variant::nil())
            },
            move |value| {
                let mut dict = setter.get().try_to::<VarDictionary>().unwrap_or_default();
                dict.set(&GString::from(key_set.as_str()), &value);
                setter.set(dict.to_variant());
            },
        )
    }
}

/// The top-level objective field value (`_field_row`'s `set` closure).
fn objective_field_value(node: &Gd<MindDialog>, index: i64, field_name: &str) -> ObjectiveValue {
    let node_get = node.clone();
    let node_set = node.clone();
    let name = String::from(field_name);
    let name_get = name.clone();
    ObjectiveValue::new(
        move || {
            objective_at(&node_get, index)
                .get(&GString::from(name_get.as_str()))
                .unwrap_or(Variant::nil())
        },
        move |value| objective_set_field(&node_set, index, &name, value),
    )
}

/// One `Seq` element value (`_seq` per-element `set`).
fn objective_element_value(
    node: &Gd<MindDialog>,
    index: i64,
    field_name: &str,
    element: usize,
) -> ObjectiveValue {
    let node_get = node.clone();
    let node_set = node.clone();
    let name = String::from(field_name);
    let name_get = name.clone();
    ObjectiveValue::new(
        move || {
            objective_at(&node_get, index)
                .get(&GString::from(name_get.as_str()))
                .and_then(|value| value.try_to::<VarArray>().ok())
                .and_then(|array| array.get(element))
                .unwrap_or(Variant::nil())
        },
        move |value| objective_set_element(&node_set, index, &name, element, value),
    )
}

/// Writes one `Seq` element back into the objective dictionary.
fn objective_set_element(
    node: &Gd<MindDialog>,
    index: i64,
    field_name: &str,
    element: usize,
    value: Variant,
) {
    let mut array = objective_at(node, index)
        .get(&GString::from(field_name))
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default();
    if element >= array.len() {
        return;
    }
    array.set(element, &value);
    objective_set_field(node, index, field_name, array.to_variant());
}

/// The generic field interpreter (`_make_editor`).
fn objective_editor(
    node: &Gd<MindDialog>,
    index: i64,
    field_name: &str,
    value_sink: &ObjectiveValue,
    value: &Variant,
    kind: &str,
    flags: &VarArray,
    nested: bool,
) -> Gd<Control> {
    match kind {
        "string" => {
            if flags_has(flags, "logiccode") {
                if nested {
                    return factory::label(&variant_text(value)).upcast();
                }
                // `@LogicCode`: Java renders only the pencil button.
                let mut pencil = factory::icon_button(
                    "pencil",
                    "",
                    "emptyi",
                    godot::classes::box_container::AlignmentMode::BEGIN,
                );
                pencil.set_custom_minimum_size(Vector2::new(34.0, 34.0));
                let target = node.clone();
                let name = String::from(field_name);
                let callable =
                    Callable::from_fn("objective_logic_code", move |_args: &[&Variant]| {
                        objective_open_logic(&target, index, &name);
                    });
                pencil.connect("pressed", &callable);
                return pencil.upcast();
            }
            if flags_has(flags, "multiline") {
                let mut area = text_area(&variant_text(value));
                area.set_custom_minimum_size(Vector2::new(0.0, 100.0));
                let sink = value_sink.clone();
                let callable = Callable::from_fn("objective_area", move |args: &[&Variant]| {
                    let text = args
                        .first()
                        .map(|value| value.to::<GString>())
                        .unwrap_or_default();
                    sink.set(text.to_variant());
                });
                area.connect("text_changed", &callable);
                return area.upcast();
            }
            let mut edit = text_field(&variant_text(value), 0);
            let sink = value_sink.clone();
            let callable = Callable::from_fn("objective_string", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| value.to::<GString>())
                    .unwrap_or_default();
                sink.set(text.to_variant());
            });
            edit.connect("text_changed", &callable);
            edit.upcast()
        }
        "bool" => {
            let checked = as_bool(Some(value.clone()));
            let sink = value_sink.clone();
            let listener = Callable::from_fn("objective_bool", move |args: &[&Variant]| {
                let pressed = args
                    .first()
                    .map(|value| value.to::<bool>())
                    .unwrap_or(false);
                sink.set(pressed.to_variant());
            });
            mind_check("", checked, listener).upcast()
        }
        "indexbool" => {
            // `@IndexBool int`: checked means `!= -1`; writes `1`/`-1`.
            let checked = as_int(Some(value.clone())) != -1;
            let sink = value_sink.clone();
            let listener = Callable::from_fn("objective_indexbool", move |args: &[&Variant]| {
                let pressed = args
                    .first()
                    .map(|value| value.to::<bool>())
                    .unwrap_or(false);
                sink.set((if pressed { 1i64 } else { -1 }).to_variant());
            });
            mind_check("", checked, listener).upcast()
        }
        "byte" | "int" | "float" => {
            let scale = if flags_has(flags, "second") {
                60.0
            } else if flags_has(flags, "tilepos") {
                8.0
            } else {
                1.0
            };
            objective_number_field(value_sink, value, kind == "float", scale)
        }
        "team" => {
            if nested {
                return factory::label(&team_name_by_id(as_int(Some(value.clone())))).upcast();
            }
            let mut button = factory::button(&team_name_by_id(as_int(Some(value.clone()))));
            let target = node.clone();
            let name = String::from(field_name);
            let callable = Callable::from_fn("objective_team", move |_args: &[&Variant]| {
                objective_open_team_picker(&target, index, &name);
            });
            button.connect("pressed", &callable);
            button.upcast()
        }
        "color" => {
            if nested {
                // A marker's colour is edited in place (`ColorPickerButton`);
                // Java's shared `ui.picker` carries no nested sink.
                let mut picker = ColorPickerButton::new_alloc();
                picker.set_custom_minimum_size(Vector2::new(50.0, 50.0));
                picker.set_pick_color(color_from_hex(&variant_text(value)));
                let sink = value_sink.clone();
                let callable =
                    Callable::from_fn("objective_marker_color", move |args: &[&Variant]| {
                        let color = args
                            .first()
                            .and_then(|value| value.try_to::<Color>().ok())
                            .unwrap_or(Color::WHITE);
                        sink.set(GString::from(hex_rgba(color).as_str()).to_variant());
                        Variant::nil()
                    });
                picker.connect("color_changed", &callable);
                return picker.upcast();
            }
            let mut button = Button::new_alloc();
            button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
            let mut swatch = ColorRect::new_alloc();
            swatch.set_color(color_from_hex(&variant_text(value)));
            swatch.set_anchors_preset(LayoutPreset::FULL_RECT);
            swatch.set_mouse_filter(MouseFilter::IGNORE);
            button.add_child(&swatch);
            let target = node.clone();
            let name = String::from(field_name);
            let current = variant_text(value);
            let callable =
                Callable::from_fn("objective_color_pick", move |_args: &[&Variant]| {
                    objective_open_color_picker(&target, &current, &name);
                });
            button.connect("pressed", &callable);
            button.upcast()
        }
        "vec2i" | "vec2f" => {
            let is_float = kind == "vec2f";
            let scale = if flags_has(flags, "tilepos") {
                8.0
            } else {
                1.0
            };
            let mut row = HBoxContainer::new_alloc();
            for axis in ["x", "y"] {
                let axis_sink = value_sink.field(axis);
                let raw = as_float(Some(axis_sink.get()));
                let display = if is_float {
                    format!("{}", raw / scale)
                } else {
                    format!("{}", (raw / scale) as i64)
                };
                let mut edit = text_field(&display, 0);
                edit.set_custom_minimum_size(Vector2::new(120.0, 0.0));
                let callable = Callable::from_fn("objective_vec2", move |args: &[&Variant]| {
                    let text = args
                        .first()
                        .map(|value| value.to::<GString>().to_string())
                        .unwrap_or_default();
                    let Some(number) = valid_float(&text) else {
                        return;
                    };
                    let stored = if is_float {
                        (number * scale).to_variant()
                    } else {
                        ((number * scale) as i64).to_variant()
                    };
                    axis_sink.set(stored);
                });
                edit.connect("text_changed", &callable);
                row.add_child(&edit);
            }
            row.upcast()
        }
        "alignment" => {
            let mut button = factory::button(&objective_align_name(as_int(Some(value.clone()))));
            let target = node.clone();
            let flags = flags.clone();
            let sink = value_sink.clone();
            let callable = Callable::from_fn("objective_alignment", move |_args: &[&Variant]| {
                objective_open_align_picker(&target, &sink, &flags);
            });
            button.connect("pressed", &callable);
            button.upcast()
        }
        "labelflag" => {
            // `@LabelFlag byte` (`:309-325`): the two `WorldLabel` flag checks.
            let mut box_ = VBoxContainer::new_alloc();
            let current = as_int(Some(value.clone()));
            for (bit, key) in [(1i64, "@marker.background"), (2, "@marker.outline")] {
                let sink = value_sink.clone();
                let listener =
                    Callable::from_fn("objective_labelflag", move |args: &[&Variant]| {
                        let pressed = args
                            .first()
                            .map(|value| value.to::<bool>())
                            .unwrap_or(false);
                        let current = as_int(Some(sink.get()));
                        let updated = if pressed {
                            current | bit
                        } else {
                            current & !bit
                        };
                        sink.set(updated.to_variant());
                    });
                box_.add_child(&mind_check(
                    &kit::t(key).to_string(),
                    (current & bit) == bit,
                    listener,
                ));
            }
            box_.upcast()
        }
        "texture" => {
            // `TextureHolder` interpreter: the string-backed name.
            let string_sink = value_sink.field("string");
            let mut edit = text_field(&variant_text(&string_sink.get()), 0);
            let callable = Callable::from_fn("objective_texture", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| value.to::<GString>())
                    .unwrap_or_default();
                string_sink.set(text.to_variant());
            });
            edit.connect("text_changed", &callable);
            edit.upcast()
        }
        "marker" => objective_marker_editor(node, value_sink),
        "verts" => objective_verts_editor(value_sink, value),
        _ if kind.starts_with("content:") => {
            if nested {
                return factory::label(&variant_text(value)).upcast();
            }
            let content_kind = kind.trim_start_matches("content:").to_string();
            let mut button = factory::button(&variant_text(value));
            let target = node.clone();
            let name = String::from(field_name);
            let callable = Callable::from_fn("objective_content", move |_args: &[&Variant]| {
                objective_open_content_picker(&target, index, &name, &content_kind);
            });
            button.connect("pressed", &callable);
            button.upcast()
        }
        // `map:` and any future descriptor kinds still render verbatim; every
        // objective/marker class field the descriptors declare has a widget.
        _ => factory::label(&variant_text(value)).upcast(),
    }
}

/// `_number_value_field(is_float, scale, get, set)` (`@Second` x60, `@TilePos` x8).
fn objective_number_field(
    value_sink: &ObjectiveValue,
    value: &Variant,
    is_float: bool,
    scale: f64,
) -> Gd<Control> {
    let raw = as_float(Some(value.clone()));
    let display = if is_float {
        format!("{}", raw / scale)
    } else {
        format!("{}", (raw / scale) as i64)
    };
    let mut edit = text_field(&display, 0);
    edit.set_custom_minimum_size(Vector2::new(120.0, 0.0));
    let sink = value_sink.clone();
    let callable = Callable::from_fn("objective_number", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| value.to::<GString>().to_string())
            .unwrap_or_default();
        if text.is_empty() || text == "-" {
            return;
        }
        let stored = if is_float {
            match valid_float(&text) {
                Some(number) => (number * scale).to_variant(),
                None => return,
            }
        } else {
            match valid_int(&text) {
                Some(number) => (number as f64 * scale).to_variant(),
                None => return,
            }
        };
        sink.set(stored);
    });
    edit.connect("text_changed", &callable);
    edit.upcast()
}

/// The `Seq` interpreter (`:335-402`): name header + add / per-element rows.
///
/// Element rows now carry the real interpreter for the element kind (string /
/// vec2 / `ObjectiveMarker`) instead of the verbatim label (GAPS #46).
fn objective_seq_editor(
    node: &Gd<MindDialog>,
    index: i64,
    field_name: &str,
    inner_kind: &str,
    inner_flags: &VarArray,
) -> Gd<Control> {
    let mut box_ = VBoxContainer::new_alloc();
    let mut header = HBoxContainer::new_alloc();
    header.add_theme_constant_override("separation", 4);
    if !field_name.is_empty() {
        let mut label = factory::label(&format!("{field_name}:"));
        label.add_theme_color_override("font_color", styles::ACCENT);
        header.add_child(&as_control(&label));
    }
    let mut spacer = Control::new_alloc();
    spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
    spacer.set_mouse_filter(MouseFilter::IGNORE);
    header.add_child(&spacer);
    let target = node.clone();
    let name = String::from(field_name);
    let inner = String::from(inner_kind);
    let callable = Callable::from_fn("objective_seq_add", move |_args: &[&Variant]| {
        let mut array = objective_at(&target, index)
            .get(&GString::from(name.as_str()))
            .and_then(|value| value.try_to::<VarArray>().ok())
            .unwrap_or_default();
        array.push(&objective_seq_default(&inner));
        objective_set_field(&target, index, &name, array.to_variant());
        request_refresh(&target, OBJ_REQUEST_PANEL);
    });
    header.add_child(&objective_small_button("add", callable));
    box_.add_child(&header);
    let array = objective_at(node, index)
        .get(&GString::from(field_name))
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default();
    for element in 0..array.len() {
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 4);
        let target = node.clone();
        let name = String::from(field_name);
        let remover = Callable::from_fn("objective_seq_remove", move |_args: &[&Variant]| {
            let mut array = objective_at(&target, index)
                .get(&GString::from(name.as_str()))
                .and_then(|value| value.try_to::<VarArray>().ok())
                .unwrap_or_default();
            if element < array.len() {
                array.remove(element);
                objective_set_field(&target, index, &name, array.to_variant());
                request_refresh(&target, OBJ_REQUEST_PANEL);
            }
        });
        row.add_child(&objective_small_button("cancel", remover));
        let element_sink = objective_element_value(node, index, field_name, element);
        let element_value = element_sink.get();
        let editor = objective_editor(
            node,
            index,
            field_name,
            &element_sink,
            &element_value,
            inner_kind,
            inner_flags,
            true,
        );
        let mut editor_control: Gd<Control> = editor;
        editor_control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        row.add_child(&editor_control);
        box_.add_child(&row);
    }
    box_.upcast()
}

/// The `ObjectiveMarker` interpreter (`MapObjectivesDialog.defaultInterpreter`
/// + `getProvider(ObjectiveMarker.class)`, `:236-256`): the class button plus
/// the selected marker class's field rows.
fn objective_marker_editor(node: &Gd<MindDialog>, value_sink: &ObjectiveValue) -> Gd<Control> {
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 2);
    let value = value_sink
        .get()
        .try_to::<VarDictionary>()
        .unwrap_or_default();
    let tag = dict_text(&value, "class");
    let mut class_button = factory::button(&objective_marker_name(&tag));
    class_button.set_theme_type_variation("flatt");
    class_button.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let target = node.clone();
    let sink = value_sink.clone();
    let callable = Callable::from_fn("objective_marker_class", move |_args: &[&Variant]| {
        objective_open_marker_picker(&target, &sink);
    });
    class_button.connect("pressed", &callable);
    box_.add_child(&class_button);
    for field in objective_marker_fields(&tag).iter_shared() {
        let Ok(field) = field.try_to::<VarDictionary>() else {
            continue;
        };
        if !objective_visible_field(&field) {
            continue;
        }
        let field_name = dict_text(&field, "name");
        if field_name == "class" || field_name == "editorPos" {
            continue;
        }
        let kind = dict_text(&field, "kind");
        let flags = dict_array(&field, "flags");
        let field_sink = value_sink.field(&field_name);
        let current = field_sink.get();
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 4);
        row.add_child(&as_control(&factory::label(&format!("{field_name}:"))));
        let editor = objective_editor(
            node,
            -1,
            &field_name,
            &field_sink,
            &current,
            &kind,
            &flags,
            true,
        );
        let mut editor_control: Gd<Control> = editor;
        editor_control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        row.add_child(&editor_control);
        box_.add_child(&row);
    }
    box_.upcast()
}

/// The marker class table (`MindEditor.marker_classes_json`) filtered by tag.
fn objective_marker_fields(tag: &str) -> VarArray {
    let classes = editor_array("marker_classes_json");
    for entry in dict_rows(&classes) {
        if dict_text(&entry, "tag") == tag {
            return dict_array(&entry, "fields");
        }
    }
    VarArray::new()
}

/// `getProvider(ObjectiveMarker.class)`: the `@add` marker-class grid; picking
/// replaces the marker with Java's provider defaults (`marker_default_json`).
fn objective_open_marker_picker(node: &Gd<MindDialog>, value_sink: &ObjectiveValue) {
    let classes = editor_array("marker_classes_json");
    if classes.is_empty() {
        return;
    }
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(3);
    grid.add_theme_constant_override("h_separation", 4);
    grid.add_theme_constant_override("v_separation", 4);
    for row in dict_rows(&classes) {
        let tag = dict_text(&row, "tag");
        let mut button = factory::button(&objective_marker_name(&tag));
        button.set_theme_type_variation("flatt");
        button.set_text_alignment(HorizontalAlignment::LEFT);
        button.set_custom_minimum_size(Vector2::new(195.0, 56.0));
        let target = node.clone();
        let sink = value_sink.clone();
        let callable = Callable::from_fn("objective_pick_marker", move |_args: &[&Variant]| {
            let created = editor_json(
                "marker_default_json",
                &[GString::from(tag.as_str()).to_variant()],
            )
            .map(|text| json_object(&text))
            .unwrap_or_default();
            if created.is_empty() {
                return Variant::nil();
            }
            sink.set(created.to_variant());
            free_named(&target, OBJ_OVERLAY);
            request_refresh(&target, OBJ_REQUEST_PANEL);
            Variant::nil()
        });
        button.connect("pressed", &callable);
        grid.add_child(&button);
    }
    let title = kit::t("@add").to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        OBJ_OVERLAY,
        &title,
        640.0,
        380.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.8),
    ) {
        overlay.add_content(&grid);
    }
}

/// `Vertices` interpreter (`MapObjectivesDialog.java:280-320`): one row per
/// vertex with `x`/`y` (tile-scaled), the packed colour and `u`/`v`. The colour
/// column stores `Color.toFloatBits()` bits (ABGR); alpha is clamped to 254 so
/// the JSON number never becomes NaN.
fn objective_verts_editor(value_sink: &ObjectiveValue, value: &Variant) -> Gd<Control> {
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 2);
    let array = value.try_to::<VarArray>().unwrap_or_default();
    let vertices = array.len() / OBJECTIVE_VERTEX_STRIDE;
    for vertex in 0..vertices {
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 4);
        for (component, name) in [(0usize, "x"), (1, "y")] {
            let sink = objective_vertex_value(value_sink, vertex, component);
            let display = format!("{}", as_float(Some(sink.get())) / OBJECTIVE_TILE_SIZE);
            let mut edit = text_field(&display, 0);
            edit.set_custom_minimum_size(Vector2::new(70.0, 0.0));
            edit.set_tooltip_text(&GString::from(name));
            let callable = Callable::from_fn("objective_vert_xy", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| value.to::<GString>().to_string())
                    .unwrap_or_default();
                let Some(number) = valid_float(&text) else {
                    return;
                };
                sink.set((number * OBJECTIVE_TILE_SIZE).to_variant());
            });
            edit.connect("text_changed", &callable);
            row.add_child(&edit);
        }
        let color_sink = objective_vertex_value(value_sink, vertex, 2);
        let mut picker = ColorPickerButton::new_alloc();
        picker.set_custom_minimum_size(Vector2::new(40.0, 40.0));
        picker.set_pick_color(objective_verts_color(as_float(Some(color_sink.get()))));
        let callable = Callable::from_fn("objective_vert_color", move |args: &[&Variant]| {
            let color = args
                .first()
                .and_then(|value| value.try_to::<Color>().ok())
                .unwrap_or(Color::WHITE);
            color_sink.set(objective_verts_bits(color).to_variant());
            Variant::nil()
        });
        picker.connect("color_changed", &callable);
        row.add_child(&picker);
        for (component, name) in [(3usize, "u"), (4, "v")] {
            let sink = objective_vertex_value(value_sink, vertex, component);
            let display = format!("{}", as_float(Some(sink.get())));
            let mut edit = text_field(&display, 0);
            edit.set_custom_minimum_size(Vector2::new(70.0, 0.0));
            edit.set_tooltip_text(&GString::from(name));
            let callable = Callable::from_fn("objective_vert_uv", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| value.to::<GString>().to_string())
                    .unwrap_or_default();
                let Some(number) = valid_float(&text) else {
                    return;
                };
                sink.set(number.to_variant());
            });
            edit.connect("text_changed", &callable);
            row.add_child(&edit);
        }
        box_.add_child(&row);
    }
    box_.upcast()
}

/// `Vertices` layout: `x, y, color, u, v, ?` per vertex (`stride = 6`).
const OBJECTIVE_VERTEX_STRIDE: usize = 6;
/// `Vars.tilesize`: the vertex `x`/`y` columns are tile-scaled.
const OBJECTIVE_TILE_SIZE: f64 = 8.0;

/// One `Vertices` component value.
fn objective_vertex_value(
    value_sink: &ObjectiveValue,
    vertex: usize,
    component: usize,
) -> ObjectiveValue {
    let getter = value_sink.clone();
    let setter = value_sink.clone();
    ObjectiveValue::new(
        move || {
            getter
                .get()
                .try_to::<VarArray>()
                .ok()
                .and_then(|array| array.get(vertex * OBJECTIVE_VERTEX_STRIDE + component))
                .unwrap_or(0.0f64.to_variant())
        },
        move |value| {
            let mut array = setter.get().try_to::<VarArray>().ok().unwrap_or_default();
            let index = vertex * OBJECTIVE_VERTEX_STRIDE + component;
            if index >= array.len() {
                return;
            }
            array.set(index, &value);
            setter.set(array.to_variant());
        },
    )
}

/// `Color.toFloatBits()` ABGR bits -> `Color` (`Vertices` colour column).
fn objective_verts_color(bits: f64) -> Color {
    let bits = (bits as f32).to_bits();
    Color::from_rgba8(
        (bits & 0xff) as u8,
        ((bits >> 8) & 0xff) as u8,
        ((bits >> 16) & 0xff) as u8,
        ((bits >> 24) & 0xff) as u8,
    )
}

/// `Color` -> the `Vertices` colour column (finite JSON number; alpha 254 max).
fn objective_verts_bits(color: Color) -> f64 {
    let alpha = u32::from(hex_byte(color.a).min(254));
    let bits = (alpha << 24)
        | (u32::from(hex_byte(color.b)) << 16)
        | (u32::from(hex_byte(color.g)) << 8)
        | u32::from(hex_byte(color.r));
    f64::from(f32::from_bits(bits))
}

/// `#rrggbbaa` for a `Color` (`objective_marker_color` write-back).
fn hex_rgba(color: Color) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}",
        hex_byte(color.r),
        hex_byte(color.g),
        hex_byte(color.b),
        hex_byte(color.a)
    )
}

/// `_provider_default_of(kind)` (`getProvider(element).get`).
fn objective_provider_default(kind: &str) -> Variant {
    if kind.starts_with("content:") {
        return GString::from("copper").to_variant();
    }
    match kind {
        "string" => GString::from("").to_variant(),
        "bool" => false.to_variant(),
        "byte" | "int" => 0i64.to_variant(),
        "float" => 0.0f64.to_variant(),
        "vec2i" => {
            let mut value = VarDictionary::new();
            value.set("x", &0i64.to_variant());
            value.set("y", &0i64.to_variant());
            value.to_variant()
        }
        "vec2f" => {
            let mut value = VarDictionary::new();
            value.set("x", &0.0f64.to_variant());
            value.set("y", &0.0f64.to_variant());
            value.to_variant()
        }
        "team" => 1i64.to_variant(),
        "color" => GString::from("ffd37fff").to_variant(),
        "labelflag" => 3i64.to_variant(),
        "alignment" => 1i64.to_variant(),
        "texture" => {
            let mut value = VarDictionary::new();
            value.set("string", &GString::from("white").to_variant());
            value.to_variant()
        }
        "marker" => VarDictionary::new().to_variant(),
        _ => Variant::nil(),
    }
}

/// Java's `getProvider(element).get` for a new `Seq` element. The marker
/// provider opens the class picker upstream; the port appends the first marker
/// class's provider default so the element already renders its field rows (the
/// class button then changes the class).
fn objective_seq_default(kind: &str) -> Variant {
    if kind != "marker" {
        return objective_provider_default(kind);
    }
    let classes = editor_array("marker_classes_json");
    let Some(first) = dict_rows(&classes).first().map(|row| dict_text(row, "tag")) else {
        return VarDictionary::new().to_variant();
    };
    let parsed = editor_json(
        "marker_default_json",
        &[GString::from(first.as_str()).to_variant()],
    )
    .map(|text| json_object(&text))
    .unwrap_or_default();
    if parsed.is_empty() {
        VarDictionary::new().to_variant()
    } else {
        parsed.to_variant()
    }
}

/// `_align_name(align)`.
fn objective_align_name(align: i64) -> String {
    OBJ_ALIGN_NAMES
        .iter()
        .find(|(_, value)| *value == align)
        .map(|(name, _)| String::from(*name))
        .unwrap_or_else(|| String::from("center"))
}

/// `_open_align_picker(get, set, flags)`: a three-column alignment popup.
fn objective_open_align_picker(
    node: &Gd<MindDialog>,
    value_sink: &ObjectiveValue,
    flags: &VarArray,
) {
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(3);
    let current = as_int(Some(value_sink.get()));
    for (name, value) in OBJ_ALIGN_NAMES {
        if flags_has(flags, "align-nover") && value != 1 && value != 8 && value != 16 {
            continue;
        }
        if flags_has(flags, "align-nohor") && value != 1 && value != 2 && value != 4 {
            continue;
        }
        let mut choice = Button::new_alloc();
        choice.set_text(name);
        choice.set_custom_minimum_size(Vector2::new(100.0, 38.0));
        choice.set_theme_type_variation(if value == current {
            "flatTogglet"
        } else {
            "flatt"
        });
        let target = node.clone();
        let sink = value_sink.clone();
        let callable = Callable::from_fn("objective_align", move |_args: &[&Variant]| {
            sink.set(value.to_variant());
            free_named(&target, OBJ_OVERLAY);
            request_refresh(&target, OBJ_REQUEST_PANEL);
        });
        choice.connect("pressed", &callable);
        grid.add_child(&choice);
    }
    let title = kit::t("@editor.objectives").to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        OBJ_OVERLAY,
        &title,
        320.0,
        240.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.8),
    ) {
        overlay.add_content(&grid);
    }
}

/// `ui.logic.show(code, null, true, set)` for `@LogicCode` fields.
fn objective_open_logic(node: &Gd<MindDialog>, index: i64, field_name: &str) {
    if kit::ui().is_none() {
        return;
    }
    let field = String::from(field_name);
    set_meta_text(&mut node.clone(), OBJ_LOGIC_FIELD_META, &field);
    objective_bind_logic(node);
    let code = objective_at(node, index)
        .get(&GString::from(field.as_str()))
        .map(|value| variant_text(&value))
        .unwrap_or_default();
    let mut context = VarDictionary::new();
    context.set("filter", &true.to_variant());
    context.set("privileged", &true.to_variant());
    context.set("code", &GString::from(code.as_str()).to_variant());
    ui_open_dialog("logic", &context);
}

/// Binds the `logic` dialog's `code_edited` signal (once).
fn objective_bind_logic(node: &Gd<MindDialog>) {
    if node.has_meta(OBJ_LOGIC_BOUND_META) {
        return;
    }
    let Some(mut logic) = dialog_node("logic") else {
        return;
    };
    if !logic.has_signal("code_edited") {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("objective_logic_edited", move |args: &[&Variant]| {
        let code = args
            .first()
            .map(|value| value.to::<GString>())
            .unwrap_or_default();
        let field = meta_text(&target, OBJ_LOGIC_FIELD_META);
        let index = meta_int(&target, OBJ_SELECTED_META, -1);
        if index < 0 || field.is_empty() {
            return;
        }
        objective_set_field(&target, index, &field, code.to_variant());
        request_refresh(&target, OBJ_REQUEST_PANEL);
    });
    logic.connect("code_edited", &callable);
    node.clone()
        .set_meta(OBJ_LOGIC_BOUND_META, &true.to_variant());
}

// ---------------------------------------------------------------------------
// MapLocalesDialog (`map_locales`)
// ---------------------------------------------------------------------------

/// Placeholder controller (fleshed out below).
pub struct MapLocalesDialog;

impl DialogController for MapLocalesDialog {}

// __APPEND__

#[cfg(test)]
mod tests {
    use super::*;
    use godot::builtin::ColorChannelOrder;

    #[test]
    fn verts_color_bits_decode_the_java_packing() {
        // `default_quad_vertices` stores `Color.white.toFloatBits()`, clamped to
        // alpha 254 so the number stays finite (`0xfeffffff`).
        let white = objective_verts_color(f64::from(f32::from_bits(0xfeff_ffff)));
        assert_eq!(
            white.to_u32(ColorChannelOrder::RGBA),
            Color::from_rgba8(255, 255, 255, 254).to_u32(ColorChannelOrder::RGBA)
        );
    }

    #[test]
    fn verts_color_bits_round_trip_without_nan() {
        for color in [
            Color::from_rgba8(255, 255, 255, 255),
            Color::from_rgba8(0x7f, 0x59, 0x9a, 0xff),
            Color::from_rgba8(0, 0, 0, 0),
        ] {
            let bits = objective_verts_bits(color);
            assert!(bits.is_finite(), "verts colour bits must stay finite");
            let back = objective_verts_color(bits);
            // Alpha is clamped to 254 (`toFloatBits` 0xffffffff is NaN).
            let expected_alpha = hex_byte(color.a).min(254);
            assert_eq!(
                back.to_u32(ColorChannelOrder::RGBA),
                Color::from_rgba8(
                    hex_byte(color.r),
                    hex_byte(color.g),
                    hex_byte(color.b),
                    expected_alpha
                )
                .to_u32(ColorChannelOrder::RGBA)
            );
        }
    }
}
