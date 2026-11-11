// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared helpers of the editor, picker and schematic dialogs.
//!
//! One leaf for the three helper modules of the old `batch_d`, `batch_e` and
//! `batch_f` (`util_d`/`util_e`/`util_f`) merged by W2-H05: the surviving
//! helpers keep their bodies, the identical copies are gone, and the shared
//! implementations come from `dialogs::common` (re-exported below) and
//! `crate::gdutil::variant`.
//! Upstream: none (port-only; shared helpers of the old batch_d/batch_e/batch_f)

use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::object::ConnectFlags;
use godot::classes::text_edit::LineWrappingMode;
use godot::classes::{
    Button, CenterContainer, ColorRect, Control, HBoxContainer, InputEvent, InputEventMouseButton,
    Json, LineEdit, MarginContainer, Node, PanelContainer, StyleBox, StyleBoxFlat, StyleBoxTexture,
    TextEdit, VBoxContainer,
};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use crate::gdutil::variant::{to_bool_strict, to_f64_boolish, to_i64_no_text, to_text_stringify};
use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::MindScroll;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::check::MindCheck;
use crate::ui::kit::widgets::factory;

// Adopted from F2's `dialogs/common`: no local copy remains in this group. The
// re-export keeps the group's `util::*` globs resolving; C01 deletes this leaf
// and repoints the imports at the shared modules.
pub(in crate::ui::dialogs) use crate::ui::dialogs::common::defer::defer_action;
pub(in crate::ui::dialogs) use crate::ui::dialogs::common::facade::{rich, ui_call};

// ---------------------------------------------------------------------------
// shared dialog helpers (old util_d)
// ---------------------------------------------------------------------------

/// `MindUi.open_dialog(name, ctx)`.
pub(in crate::ui::dialogs) fn open_named_dialog(name: &str, context_json: &str) {
    let _ = ui_call(
        "open_dialog",
        &[
            GString::from(name).to_variant(),
            GString::from(context_json).to_variant(),
        ],
    );
}

/// `MindUi.close_dialog(name)`.
pub(in crate::ui::dialogs) fn close_named_dialog(name: &str) {
    let _ = ui_call("close_dialog", &[GString::from(name).to_variant()]);
}

/// `ui.showInfo(text)`.
pub(in crate::ui::dialogs) fn notify_info(text: &str) {
    if text.is_empty() {
        return;
    }
    let _ = ui_call("show_info", &[GString::from(text).to_variant()]);
}

/// `ui.showInfo(_t(key))`.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn notify_key_info(key: &str) {
    notify_info(&kit::t(key).to_string());
}

/// `ui.showErrorMessage(text)` via the shared text prompt (`@error.title`).
pub(in crate::ui::dialogs) fn notify_error(text: &str) {
    let title = kit::markup_text(&kit::t("@error.title").to_string());
    let body = kit::markup_text(text);
    let _ = ui_call("show_text", &[title.to_variant(), body.to_variant()]);
}

/// `ui.showErrorMessage(_t(key))`.
pub(in crate::ui::dialogs) fn notify_error_key(key: &str) {
    notify_error(&kit::t(key).to_string());
}

/// `Bundle.format(text, args)`: `{0}`-style substitution (`_fmt`).
fn fmt(key: &str, args: &[&str]) -> String {
    let mut result = kit::t(key).to_string();
    for (index, arg) in args.iter().enumerate() {
        result = result.replace(&format!("{{{index}}}"), arg);
    }
    result
}

/// `MindWidgets.markup_text(Bundle.format(key, args))`.
pub(in crate::ui::dialogs) fn fmt_markup(key: &str, args: &[&str]) -> GString {
    kit::markup_text(&fmt(key, args))
}

/// A dictionary value coerced to text (`str(value)`; empty when absent).
pub(in crate::ui::dialogs) fn dict_string(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| value.to::<GString>().to_string())
        .unwrap_or_default()
}

/// A dictionary value coerced to a bool.
pub(in crate::ui::dialogs) fn dict_bool(dict: &VarDictionary, key: &str, default: bool) -> bool {
    dict.get(key)
        .map(|value| value.to::<bool>())
        .unwrap_or(default)
}

/// `JSON.stringify(dictionary)`.
pub(in crate::ui::dialogs) fn json_stringify(value: &VarDictionary) -> String {
    Json::stringify(&value.to_variant()).to_string()
}

/// Reads and clears one string meta key (`""` when unset).
pub(in crate::ui::dialogs) fn take_meta_string(node: &mut Gd<MindDialog>, key: &str) -> String {
    let name = StringName::from(key);
    if !node.has_meta(&name) {
        return String::new();
    }
    let value = node.get_meta(&name).to::<GString>().to_string();
    node.remove_meta(&name);
    value
}

/// Reads and clears one bool meta key (`false` when unset).
pub(in crate::ui::dialogs) fn take_meta_bool(node: &mut Gd<MindDialog>, key: &str) -> bool {
    let name = StringName::from(key);
    if !node.has_meta(&name) {
        return false;
    }
    let value = node.get_meta(&name).to::<bool>();
    node.remove_meta(&name);
    value
}

/// Reads one string meta key (`""` when unset).
pub(in crate::ui::dialogs) fn meta_string(node: &Gd<MindDialog>, key: &str) -> String {
    let name = StringName::from(key);
    if !node.has_meta(&name) {
        return String::new();
    }
    node.get_meta(&name).to::<GString>().to_string()
}

/// `BaseDialog.close()` from an engine callback.
pub(in crate::ui::dialogs) fn close_node(target: &Gd<MindDialog>) {
    if !target.is_instance_valid() {
        return;
    }
    let mut node = target.clone();
    node.bind_mut().close();
}

/// A `pressed` listener closing the dialog (`_close_pressed`).
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn close_press(target: &Gd<MindDialog>) -> Callable {
    let target = target.clone();
    Callable::from_fn("batch_d_close", move |_args: &[&Variant]| {
        close_node(&target);
    })
}

/// One-shot `file_chosen` route for the fallback chooser (`_chooser_action`).
pub(in crate::ui::dialogs) const CHOOSER_ACTION_META: &str = "_batch_d_chooser_action";
/// The installed `file_chosen` callable, so a re-open replaces it
/// (`_chooser_connected`).
const CHOOSER_CONNECTED_META: &str = "_batch_d_chooser_connected";

/// A `file_chosen` listener for the fallback chooser.
///
/// `EditorMapsDialog`/`editor_import`/`editor_export` connect it before they
/// open `file_chooser` and drop it again on the first pick
/// (`_connect_chooser(true/false)`); `ConnectFlags::ONE_SHOT` is the same
/// lifecycle, and the stored callable replaces a connection left over from a
/// cancelled chooser.
fn connect_file_chosen(
    target: &Gd<MindDialog>,
    handler: impl Fn(&mut Gd<MindDialog>, &str) + 'static,
) {
    let Some(mut chooser) = dialog_node("file_chooser") else {
        return;
    };
    if !chooser.has_signal("file_chosen") {
        return;
    }
    let mut node = target.clone();
    let connected = StringName::from(CHOOSER_CONNECTED_META);
    if node.has_meta(&connected) {
        if let Ok(previous) = node.get_meta(&connected).try_to::<Callable>()
            && chooser.is_connected("file_chosen", &previous)
        {
            chooser.disconnect("file_chosen", &previous);
        }
        node.remove_meta(&connected);
    }
    let mut dialog = target.clone();
    let callable = Callable::from_fn("batch_d_file_chosen", move |args: &[&Variant]| {
        let path = args
            .first()
            .and_then(|value| value.try_to::<GString>().ok())
            .unwrap_or_default();
        handler(&mut dialog, &path.to_string());
    });
    chooser.connect_flags("file_chosen", &callable, ConnectFlags::ONE_SHOT);
    node.set_meta(&connected, &callable.to_variant());
}

/// `FileChooser.open(extension)` context (`{"open": …, "extensions": […]}`).
pub(in crate::ui::dialogs) fn chooser_context(is_open: bool, extensions: &[&str]) -> String {
    let mut context = VarDictionary::new();
    context.set("open", &is_open.to_variant());
    let mut list = VarArray::new();
    for extension in extensions {
        list.push(&GString::from(*extension).to_variant());
    }
    context.set("extensions", &list.to_variant());
    json_stringify(&context)
}

/// `_open_chooser`: opens the fallback chooser and hands one pick to `handler`.
pub(in crate::ui::dialogs) fn open_chooser_route(
    target: &Gd<MindDialog>,
    action: &str,
    context_json: &str,
    handler: impl Fn(&mut Gd<MindDialog>, &str) + 'static,
) {
    let mut node = target.clone();
    node.set_meta(CHOOSER_ACTION_META, &GString::from(action).to_variant());
    connect_file_chosen(target, handler);
    open_named_dialog("file_chooser", context_json);
}

/// `String.get_extension()`: the text after the last `.` of the file name.
pub(in crate::ui::dialogs) fn file_extension(file_name: &str) -> String {
    let name = match file_name.rsplit_once('/') {
        Some((_, name)) => name,
        None => file_name,
    };
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => extension.to_string(),
        _ => String::new(),
    }
}

/// `String.path_join(file)`: joins with one separator, keeping Godot schemes.
pub(in crate::ui::dialogs) fn path_join(base: &str, file: &str) -> String {
    if file.is_empty() {
        return base.to_string();
    }
    if base.is_empty() {
        return file.to_string();
    }
    if file.starts_with('/') || file.contains("://") {
        return file.to_string();
    }
    if base.ends_with('/') {
        return format!("{base}{file}");
    }
    format!("{base}/{file}")
}

/// `String.simplify_path()`: collapses `.`/`..`/duplicate separators.
pub(in crate::ui::dialogs) fn simplify_path(path: &str) -> String {
    let (scheme, rest) = match path.find("://") {
        Some(index) => (&path[..index + 3], &path[index + 3..]),
        None => ("", path),
    };
    let absolute = rest.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|last| *last != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push(part);
                }
            }
            _ => parts.push(part),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("{scheme}/{joined}")
    } else {
        format!("{scheme}{joined}")
    }
}

// ---------------------------------------------------------------------------
// confirm / text-input prompt wiring
// ---------------------------------------------------------------------------

/// The map whose info page is open (`_name`), read by the confirm handler.
pub(in crate::ui::dialogs) const EDITOR_MAP_NAME_META: &str = "_batch_d_editor_map_name";
/// A `@delete` confirm is awaiting an answer (`_pending_delete`).
pub(in crate::ui::dialogs) const EDITOR_MAP_DELETE_META: &str = "_batch_d_editor_map_delete";

/// Connects a one-shot-style `MindUi.confirm_result` handler once per dialog.
///
/// The answer is queued ([`defer_action`]) because `MindUi` emits the signal
/// from inside its own method; the handler re-binds the dialog afterwards.
pub(in crate::ui::dialogs) fn connect_confirm_prompt(
    dialog: &mut MindDialog,
    key: &str,
    handler: impl Fn(&Gd<MindDialog>, bool) + 'static,
) {
    let meta = format!("_batch_d_{key}_confirm");
    if dialog.base().has_meta(&meta) {
        return;
    }
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_signal("confirm_result") {
        return;
    }
    let target = dialog.to_gd();
    let handler = Rc::new(handler);
    let callable = Callable::from_fn("batch_d_confirm_result", move |args: &[&Variant]| {
        let confirmed = args
            .first()
            .and_then(|value| value.try_to::<bool>().ok())
            .unwrap_or(false);
        let target = target.clone();
        let handler = handler.clone();
        defer_action("batch_d_confirm_deferred", move || {
            handler(&target, confirmed);
        });
    });
    ui.connect("confirm_result", &callable);
    dialog.base_mut().set_meta(&meta, &true.to_variant());
}

/// Connects a `MindUi.text_input_result` handler once per dialog.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn connect_text_input_prompt(
    dialog: &mut MindDialog,
    key: &str,
    handler: impl Fn(&Gd<MindDialog>, &str) + 'static,
) {
    let meta = format!("_batch_d_{key}_text_input");
    if dialog.base().has_meta(&meta) {
        return;
    }
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_signal("text_input_result") {
        return;
    }
    let target = dialog.to_gd();
    let handler = Rc::new(handler);
    let callable = Callable::from_fn("batch_d_text_input_result", move |args: &[&Variant]| {
        let text = args
            .first()
            .and_then(|value| value.try_to::<GString>().ok())
            .unwrap_or_default();
        let target = target.clone();
        let handler = handler.clone();
        defer_action("batch_d_text_input_deferred", move || {
            handler(&target, &text.to_string());
        });
    });
    ui.connect("text_input_result", &callable);
    dialog.base_mut().set_meta(&meta, &true.to_variant());
}

/// A grid rebuild was requested by another editor dialog (`_rebuild()`);
/// consumed by `EditorMapsDialog::shown`.
const MAPS_REBUILD_META: &str = "_batch_d_maps_rebuild";

/// `EditorMapsDialog._rebuild()` from another dialog (delete / import).
pub(in crate::ui::dialogs) fn editor_maps_request_rebuild() {
    let Some(mut maps) = dialog_node("editor_maps") else {
        return;
    };
    maps.set_meta(MAPS_REBUILD_META, &true.to_variant());
    let _ = maps.call_deferred("refresh_controller", &[]);
}

// ---------------------------------------------------------------------------
// shared plumbing (old util_e)
// ---------------------------------------------------------------------------

/// `/root/Spine/Ui/UiRoot` (the dialog registry the GDScript used).
const UI_ROOT_PATH: &str = "/root/Spine/Ui/UiRoot";
/// Ninepatch split of `Tex.button` (`MindThemeBuilder._button_nine_patch`).
const BUTTON_SPLIT: f32 = 12.0;

pub(in crate::ui::dialogs) fn dict_text(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| to_text_stringify(&value))
        .unwrap_or_default()
}

pub(in crate::ui::dialogs) fn dict_int(dict: &VarDictionary, key: &str) -> i64 {
    to_i64_no_text(dict.get(key))
}

pub(in crate::ui::dialogs) fn dict_float(dict: &VarDictionary, key: &str, fallback: f32) -> f32 {
    match dict.get(key) {
        Some(value) => to_f64_boolish(Some(value)) as f32,
        None => fallback,
    }
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
    to_text_stringify(&node.get_meta(key))
}

pub(in crate::ui::dialogs) fn meta_int(node: &Gd<MindDialog>, key: &str, fallback: i64) -> i64 {
    if !node.has_meta(key) {
        return fallback;
    }
    to_i64_no_text(Some(node.get_meta(key)))
}

pub(in crate::ui::dialogs) fn meta_bool(node: &Gd<MindDialog>, key: &str) -> bool {
    node.has_meta(key) && to_bool_strict(Some(node.get_meta(key)))
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
        .map(|value| to_bool_strict(Some(value)))
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
pub(in crate::ui::dialogs) fn flat_box_plain(fill: Color) -> Gd<StyleBoxFlat> {
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
    let mut box_ = factory::vbox(6);
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

// ---------------------------------------------------------------------------
// shared helpers (old util_f)
// ---------------------------------------------------------------------------

/// `Vars.mobile`-independent portrait test (`Core.graphics.isPortrait()`).
pub(in crate::ui::dialogs) fn is_portrait(dialog: &MindDialog) -> bool {
    let size = dialog.base().get_viewport_rect().size;
    size.y > size.x
}

/// The viewport width (`get_viewport_rect().size.x`).
pub(in crate::ui::dialogs) fn viewport_width(dialog: &MindDialog) -> f32 {
    dialog.base().get_viewport_rect().size.x
}

/// `MindWidgets.markup(text)` for a raw markup string.
pub(in crate::ui::dialogs) fn markup(raw: &str) -> String {
    kit::markup_text(raw).to_string()
}

/// Connects a `MindUi` prompt signal once.
pub(in crate::ui::dialogs) fn connect_prompt_once(
    dialog: &mut MindDialog,
    key: &str,
    signal: &str,
    callable: &Callable,
) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    let meta = format!("_batch_f_{key}");
    if dialog.base().has_meta(&meta) || !ui.has_signal(signal) {
        return;
    }
    let _ = ui.connect(signal, callable);
    dialog.base_mut().set_meta(&meta, &true.to_variant());
}

/// Re-runs the active controller's `shown` hook on a live dialog handle.
pub(in crate::ui::dialogs) fn dialog_refresh(dialog: &Gd<MindDialog>) {
    if !dialog.is_instance_valid() {
        return;
    }
    let mut node = dialog.clone();
    node.bind_mut().refresh_controller();
}

/// `JSON.stringify(value)` for one variant.
pub(in crate::ui::dialogs) fn stringify(value: &Variant) -> String {
    Json::stringify(value).to_string()
}

/// An array of strings as a Godot array (`Array[String]` writes).
pub(in crate::ui::dialogs) fn string_array_variant(values: &[String]) -> Variant {
    let mut array = VarArray::new();
    for value in values {
        array.push(&GString::from(value.as_str()).to_variant());
    }
    array.to_variant()
}

/// `String.is_valid_int()`: an optional sign followed by digits only.
pub(in crate::ui::dialogs) fn is_valid_int(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    !body.is_empty() && body.chars().all(|character| character.is_ascii_digit())
}

/// A flat `StyleBoxFlat` with a solid fill (`MindThemeBuilder.flat`).
pub(in crate::ui::dialogs) fn flat_box(fill: Color) -> Gd<StyleBoxFlat> {
    // code-instantiated: the theme builder's `flat` helper is private; the
    // gray data cards need the same look at runtime.
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(fill);
    box_.set_corner_radius_all(3);
    box_.set_content_margin(Side::LEFT, 8.0);
    box_.set_content_margin(Side::RIGHT, 8.0);
    box_.set_content_margin(Side::TOP, 4.0);
    box_.set_content_margin(Side::BOTTOM, 4.0);
    box_
}

/// A flat 50x50 `graySquarei` action cell (`_square`).
pub(in crate::ui::dialogs) fn square_cell(
    icon_name: &str,
    tooltip: &str,
    handler: Callable,
) -> Gd<Button> {
    // code-instantiated: per-row action cells are data-driven.
    let mut button = factory::icon_button(
        icon_name,
        "",
        "graySquarei",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
    button.set_tooltip_text(tooltip);
    button.connect("pressed", &handler);
    button
}

/// Shows a transient error toast (`ui.toast(text, "error")`).
pub(in crate::ui::dialogs) fn toast_error(text: &str) {
    let _ = ui_call(
        "toast",
        &[
            GString::from(text).to_variant(),
            GString::from("error").to_variant(),
        ],
    );
}

/// `_notify(key)`: the bundle lookup as an error toast.
pub(in crate::ui::dialogs) fn notify_key(key: &str) {
    toast_error(&kit::t(key).to_string());
}

/// `_show_text(title, body)`: the shared text prompt with a toast fallback.
pub(in crate::ui::dialogs) fn show_text(title: &str, body: &str) {
    if ui_call(
        "show_text",
        &[
            GString::from(title).to_variant(),
            GString::from(body).to_variant(),
        ],
    )
    .is_some()
    {
        return;
    }
    show_toast(body);
}

/// `MindDialog.show_toast`: a transient info prompt through `MindUi`.
pub(in crate::ui::dialogs) fn show_toast(text: &str) {
    if text.is_empty() {
        return;
    }
    let _ = ui_call("show_info", &[GString::from(text).to_variant()]);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// `false` under a plain `cargo test`: the GDExtension entry point never
    /// runs, so Godot builtins (`VarDictionary`/`VarArray`/`Json`) and the
    /// asset-host lookup behind `kit::markup_text` panic with "Godot engine
    /// not available" (PAR-0586). Engine-backed assertions in this module are
    /// skipped there; the in-engine `dialog_sweep_check` gate builds and shows
    /// the real dialogs, so the production path stays covered.
    pub(crate) fn engine_available() -> bool {
        godot::sys::is_godot_initialized()
    }
    #[test]
    fn integer_validation_matches_godot() {
        assert!(is_valid_int("123"));
        assert!(is_valid_int("-42"));
        assert!(!is_valid_int(""));
        assert!(!is_valid_int("-"));
        assert!(!is_valid_int("1.5"));
        assert!(!is_valid_int("x1"));
    }
}
