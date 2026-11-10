// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared plumbing of the old `batch_e` for the map-editor dialogs.
//! Upstream: none (port-only; shared helpers of the old batch_e)

use godot::builtin::Side;
use godot::classes::CenterContainer;
use godot::classes::ColorRect;
use godot::classes::Control;
use godot::classes::HBoxContainer;
use godot::classes::InputEvent;
use godot::classes::InputEventMouseButton;
use godot::classes::Json;
use godot::classes::LineEdit;
use godot::classes::MarginContainer;
use godot::classes::Node;
use godot::classes::PanelContainer;
use godot::classes::RichTextLabel;
use godot::classes::StyleBox;
use godot::classes::StyleBoxFlat;
use godot::classes::StyleBoxTexture;
use godot::classes::TextEdit;
use godot::classes::VBoxContainer;
use godot::classes::control::LayoutPreset;
use godot::classes::control::MouseFilter;
use godot::classes::control::SizeFlags;
use godot::classes::text_edit::LineWrappingMode;
use godot::obj::NewAlloc;
use godot::obj::NewGd;
use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::MindLabel;
use crate::ui::kit::MindScroll;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::check::MindCheck;
use crate::ui::kit::widgets::factory;

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
pub(in crate::ui::dialogs) fn request_refresh(node: &Gd<MindDialog>, request: &str) {
    let mut target = node.clone();
    target.set_meta(REQUEST_META, &GString::from(request).to_variant());
    let _ = target.call("refresh_controller", &[]);
}

/// Deferred [`request_refresh`] for callbacks that run inside a `MindUi` borrow.
pub(in crate::ui::dialogs) fn request_refresh_deferred(node: &Gd<MindDialog>, request: &str) {
    let mut target = node.clone();
    target.set_meta(REQUEST_META, &GString::from(request).to_variant());
    let _ = target.call_deferred("refresh_controller", &[]);
}

/// Reads and clears the pending request (`""` when none is queued).
pub(in crate::ui::dialogs) fn take_request(dialog: &mut MindDialog) -> String {
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
pub(in crate::ui::dialogs) fn variant_text(value: &Variant) -> String {
    value
        .try_to::<GString>()
        .map(|text| text.to_string())
        .unwrap_or_else(|_| value.stringify().to_string())
}

/// GDScript `int(value)` for a variant.
pub(in crate::ui::dialogs) fn as_int(value: Option<Variant>) -> i64 {
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
pub(in crate::ui::dialogs) fn as_float(value: Option<Variant>) -> f64 {
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
pub(in crate::ui::dialogs) fn as_bool(value: Option<Variant>) -> bool {
    let Some(value) = value else {
        return false;
    };
    value.try_to::<bool>().unwrap_or(false)
}

pub(in crate::ui::dialogs) fn dict_text(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| variant_text(&value))
        .unwrap_or_default()
}

pub(in crate::ui::dialogs) fn dict_int(dict: &VarDictionary, key: &str) -> i64 {
    as_int(dict.get(key))
}

pub(in crate::ui::dialogs) fn dict_float(dict: &VarDictionary, key: &str, fallback: f32) -> f32 {
    match dict.get(key) {
        Some(value) => as_float(Some(value)) as f32,
        None => fallback,
    }
}

pub(in crate::ui::dialogs) fn dict_bool(dict: &VarDictionary, key: &str) -> bool {
    as_bool(dict.get(key))
}

pub(in crate::ui::dialogs) fn dict_array(dict: &VarDictionary, key: &str) -> VarArray {
    match dict.get(key) {
        Some(value) => value.try_to::<VarArray>().unwrap_or_default(),
        None => VarArray::new(),
    }
}

pub(in crate::ui::dialogs) fn dict_dict(dict: &VarDictionary, key: &str) -> VarDictionary {
    match dict.get(key) {
        Some(value) => value.try_to::<VarDictionary>().unwrap_or_default(),
        None => VarDictionary::new(),
    }
}

/// The dictionary rows of a `VarArray` (non-dictionary entries dropped).
pub(in crate::ui::dialogs) fn dict_rows(array: &VarArray) -> Vec<VarDictionary> {
    array
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

// -- JSON -------------------------------------------------------------------

/// `JSON.parse_string` of a payload (`null` for an empty string).
pub(in crate::ui::dialogs) fn json_value(text: &GString) -> Variant {
    if text.is_empty() {
        return Variant::nil();
    }
    Json::parse_string(text)
}

/// `JSON.parse_string` of a payload (`[]` when it is not an array).
pub(in crate::ui::dialogs) fn json_array(text: &GString) -> VarArray {
    json_value(text).try_to::<VarArray>().unwrap_or_default()
}

/// `JSON.parse_string` of a payload (`{}` when it is not an object).
pub(in crate::ui::dialogs) fn json_object(text: &GString) -> VarDictionary {
    json_value(text)
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// `JSON.stringify(value)`.
pub(in crate::ui::dialogs) fn json_string(value: &Variant) -> GString {
    Json::stringify(value)
}

// -- node metadata ----------------------------------------------------------

pub(in crate::ui::dialogs) fn meta_text(node: &Gd<MindDialog>, key: &str) -> String {
    if !node.has_meta(key) {
        return String::new();
    }
    variant_text(&node.get_meta(key))
}

pub(in crate::ui::dialogs) fn meta_int(node: &Gd<MindDialog>, key: &str, fallback: i64) -> i64 {
    if !node.has_meta(key) {
        return fallback;
    }
    as_int(Some(node.get_meta(key)))
}

pub(in crate::ui::dialogs) fn meta_bool(node: &Gd<MindDialog>, key: &str) -> bool {
    node.has_meta(key) && as_bool(Some(node.get_meta(key)))
}

pub(in crate::ui::dialogs) fn set_meta_text(node: &mut Gd<MindDialog>, key: &str, value: &str) {
    node.set_meta(key, &GString::from(value).to_variant());
}

pub(in crate::ui::dialogs) fn set_meta_int(node: &mut Gd<MindDialog>, key: &str, value: i64) {
    node.set_meta(key, &value.to_variant());
}

pub(in crate::ui::dialogs) fn set_meta_bool(node: &mut Gd<MindDialog>, key: &str, value: bool) {
    node.set_meta(key, &value.to_variant());
}

/// The dictionary stored under `key` (`{}` when absent/not a dictionary).
pub(in crate::ui::dialogs) fn meta_dict(node: &Gd<MindDialog>, key: &str) -> VarDictionary {
    if !node.has_meta(key) {
        return VarDictionary::new();
    }
    node.get_meta(key)
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// The array stored under `key` (`[]` when absent/not an array).
pub(in crate::ui::dialogs) fn meta_array(node: &Gd<MindDialog>, key: &str) -> VarArray {
    if !node.has_meta(key) {
        return VarArray::new();
    }
    node.get_meta(key).try_to::<VarArray>().unwrap_or_default()
}

pub(in crate::ui::dialogs) fn set_meta_dict(
    node: &mut Gd<MindDialog>,
    key: &str,
    value: &VarDictionary,
) {
    node.set_meta(key, &value.to_variant());
}

pub(in crate::ui::dialogs) fn set_meta_array(
    node: &mut Gd<MindDialog>,
    key: &str,
    value: &VarArray,
) {
    node.set_meta(key, &value.to_variant());
}

// -- editor facade ----------------------------------------------------------

/// Calls a method on `/root/Spine/MindEditor` (`None` when absent).
pub(in crate::ui::dialogs) fn editor_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut editor = kit::editor()?;
    if !editor.has_method(method) {
        return None;
    }
    Some(editor.call(method, args))
}

/// A JSON-returning editor method (`None` when absent or not a string).
pub(in crate::ui::dialogs) fn editor_json(method: &str, args: &[Variant]) -> Option<GString> {
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
pub(in crate::ui::dialogs) fn editor_array(method: &str) -> VarArray {
    editor_json(method, &[])
        .map(|text| json_array(&text))
        .unwrap_or_default()
}

/// The raw array payload of a one-argument editor method.
pub(in crate::ui::dialogs) fn editor_array_arg(method: &str, argument: &str) -> VarArray {
    editor_json(method, &[GString::from(argument).to_variant()])
        .map(|text| json_array(&text))
        .unwrap_or_default()
}

/// Writes a JSON payload through a `set_*_json` editor method.
pub(in crate::ui::dialogs) fn editor_write(method: &str, json: &GString) -> bool {
    editor_call(method, &[json.clone().to_variant()])
        .map(|value| as_bool(Some(value)))
        .unwrap_or(false)
}

/// `MindEditor.tags()` (`_tags()`).
pub(in crate::ui::dialogs) fn editor_tags() -> VarDictionary {
    editor_call("tags", &[])
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// `MindEditor.set_tag` (`_set_tag`).
pub(in crate::ui::dialogs) fn editor_set_tag(key: &str, value: &str) {
    let _ = editor_call(
        "set_tag",
        &[
            GString::from(key).to_variant(),
            GString::from(value).to_variant(),
        ],
    );
}

/// `MindEditor.palette_teams()` rows.
pub(in crate::ui::dialogs) fn palette_teams() -> Vec<VarDictionary> {
    editor_call("palette_teams", &[])
        .and_then(|value| value.try_to::<VarArray>().ok())
        .map(|array| dict_rows(&array))
        .unwrap_or_default()
}

/// The base teams' display names (`showTeamSelect`).
pub(in crate::ui::dialogs) fn palette_team_names() -> Vec<String> {
    palette_teams()
        .iter()
        .map(|row| dict_text(row, "name"))
        .collect()
}

/// `_team_id(name)` for `MapObjectivesDialog` (the row's `id` field).
pub(in crate::ui::dialogs) fn team_id_by_name(name: &str) -> i64 {
    palette_teams()
        .iter()
        .find(|row| dict_text(row, "name") == name)
        .map(|row| dict_int(row, "id"))
        .unwrap_or(-1)
}

/// `teamName(id)` for `MapObjectivesDialog`.
pub(in crate::ui::dialogs) fn team_name_by_id(id: i64) -> String {
    palette_teams()
        .iter()
        .find(|row| dict_int(row, "id") == id)
        .map(|row| dict_text(row, "name"))
        .unwrap_or_else(|| id.to_string())
}

// -- dialog registry / UI helpers -------------------------------------------

/// A dialog node by manifest name (the `UiRoot` registry, else the tree).
pub(in crate::ui::dialogs) fn dialog_node(name: &str) -> Option<Gd<Node>> {
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
pub(in crate::ui::dialogs) fn ui_open_dialog(name: &str, context: &VarDictionary) {
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
pub(in crate::ui::dialogs) fn ui_show_info(text: &GString) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("show_info") {
        return;
    }
    let _ = ui.call("show_info", &[text.clone().to_variant()]);
}

/// `MindUi.toast(text, kind)`.
pub(in crate::ui::dialogs) fn ui_toast(text: &GString, kind: &str) {
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
pub(in crate::ui::dialogs) fn ui_show_confirm(text: &str) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("show_confirm") {
        return;
    }
    let _ = ui.call("show_confirm", &[GString::from(text).to_variant()]);
}

/// `UiRoot.load_and(callback)` (a bare call when the host is absent).
pub(in crate::ui::dialogs) fn ui_root_load_and(callback: Callable) {
    if let Some(mut root) = kit::node(UI_ROOT_PATH)
        && root.has_method("load_and")
    {
        let _ = root.call("load_and", &[callback.to_variant()]);
        return;
    }
    let _ = callback.callv(&VarArray::new());
}

/// The `MindUi.settings_get_text` value.
pub(in crate::ui::dialogs) fn settings_text(key: &str, fallback: &str) -> String {
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
pub(in crate::ui::dialogs) fn available_locales() -> Vec<String> {
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
pub(in crate::ui::dialogs) fn live_locale() -> String {
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
pub(in crate::ui::dialogs) fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

/// The control handle of any `Control`-derived node.
pub(in crate::ui::dialogs) fn as_control<N: Inherits<Control>>(node: &Gd<N>) -> Gd<Control> {
    node.clone().upcast()
}

/// `MindWidgets.check(text, checked, listener)`.
pub(in crate::ui::dialogs) fn mind_check(
    text: &str,
    checked: bool,
    listener: Callable,
) -> Gd<MindCheck> {
    // code-instantiated: the check row is data-driven per settings/field row.
    let mut check = MindCheck::new_alloc();
    check
        .bind_mut()
        .setup(GString::from(text), checked, listener);
    check
}

/// `MindThemeBuilder._button_nine_patch(region)`: a `Tex.button` 9-patch.
pub(in crate::ui::dialogs) fn button_nine_patch(region: &str) -> Gd<StyleBox> {
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
pub(in crate::ui::dialogs) fn pane_frame(
    fill: Color,
    border: Color,
    radius: f32,
) -> Gd<StyleBoxFlat> {
    let mut style = MindDialog::pane_box(fill, border, 4.0, radius, -1.0);
    style.set_content_margin_all(0.0);
    style
}

/// A flat `StyleBoxFlat` with a solid fill (`_flat_box`).
pub(in crate::ui::dialogs) fn flat_box(fill: Color) -> Gd<StyleBoxFlat> {
    // code-instantiated: flat style boxes are per-cell presentation.
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(fill);
    style.set_corner_radius_all(0);
    style
}

/// A `LineEdit` carrying a value and a maximum length.
pub(in crate::ui::dialogs) fn text_field(value: &str, max_length: i32) -> Gd<LineEdit> {
    let mut field = factory::field("");
    field.set_text(&GString::from(value));
    field.set_max_length(max_length);
    field
}

/// A `TextEdit` carrying a value (the multi-line `area` fields).
pub(in crate::ui::dialogs) fn text_area(value: &str) -> Gd<TextEdit> {
    // code-instantiated: multi-line fields are data-driven per descriptor.
    let mut area = TextEdit::new_alloc();
    area.set_text(&GString::from(value));
    area.set_line_wrapping_mode(LineWrappingMode::BOUNDARY);
    area
}

/// Truncates to `max_length` characters (GDScript `substr(0, max_length)`).
pub(in crate::ui::dialogs) fn truncate_chars(text: &str, max_length: i32) -> String {
    if max_length < 0 {
        return text.to_string();
    }
    text.chars().take(max_length as usize).collect()
}

/// `text.is_valid_int()`.
pub(in crate::ui::dialogs) fn valid_int(text: &str) -> Option<i64> {
    text.trim().parse::<i64>().ok()
}

/// `text.is_valid_float()`.
pub(in crate::ui::dialogs) fn valid_float(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok()
}

/// `Point2.pack(x, y)`: two 16-bit coordinates in one int.
pub(in crate::ui::dialogs) fn pack_point2(x: i32, y: i32) -> i32 {
    // `arc Point2.pack` (`Point2.java:40-42`): x in the high half, y low.
    ((x as i16 as i32) << 16) | ((y as i16 as u16) as i32)
}

/// `Point2.x/y` unpack: both halves sign-extend (`Point2.java:44-52`).
pub(in crate::ui::dialogs) fn unpack_point2(packed: i32) -> (i32, i32) {
    ((packed >> 16) as i16 as i32, packed as i16 as i32)
}

/// A modal overlay scrim + frame added over the dialog (`_show_overlay`).
pub(in crate::ui::dialogs) struct Overlay {
    #[allow(dead_code)]
    // ported surface; not reachable from the native call graph (kept for parity)
    pub(in crate::ui::dialogs) layer: Gd<ColorRect>,
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub(in crate::ui::dialogs) frame_box: Gd<VBoxContainer>,
    scroll: Gd<MindScroll>,
}

impl Overlay {
    /// Adds a content node inside the overlay's scroll pane.
    pub(in crate::ui::dialogs) fn add_content<N: Inherits<Control>>(&self, content: &Gd<N>) {
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
pub(in crate::ui::dialogs) fn begin_overlay(
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
pub(in crate::ui::dialogs) fn close_overlay(dialog: &mut MindDialog, name: &str) {
    if let Some(mut existing) = dialog.base().get_node_or_null(name) {
        existing.queue_free();
    }
}
