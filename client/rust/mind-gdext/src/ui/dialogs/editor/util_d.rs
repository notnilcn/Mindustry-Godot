// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared helpers of the old `batch_d` for the editor, picker and dev dialogs.
//! Upstream: none (port-only; shared helpers of the old batch_d)

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::Json;
use godot::classes::Node;
use godot::classes::RichTextLabel;
use godot::classes::object::ConnectFlags;
use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::MindLabel;

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// A queued one-shot action (`Callable::call_deferred` payload).
type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// Queues `action` for the idle phase (see the module note on PAR-0066).
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

/// One `MindLabel`'s engine handle (text/colour/wrap on a rich label).
pub(in crate::ui::dialogs) fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

/// Calls a `MindUi` method (`None` when the autoload/method is absent).
pub(in crate::ui::dialogs) fn ui_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut ui = kit::ui()?;
    if !ui.has_method(method) {
        return None;
    }
    Some(ui.call(method, args))
}

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

/// A dialog node by manifest name (the `UiRoot` registry, else the tree).
pub(in crate::ui::dialogs) fn dialog_node(name: &str) -> Option<Gd<Node>> {
    if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
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

/// `ui.showInfo(text)`.
pub(in crate::ui::dialogs) fn notify_info(text: &str) {
    if text.is_empty() {
        return;
    }
    let _ = ui_call("show_info", &[GString::from(text).to_variant()]);
}

/// `ui.showInfo(_t(key))`.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn notify_key(key: &str) {
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

/// `JSON.parse_string(text)` as an object (`{}` for any other shape).
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn parse_json_object(text: &str) -> VarDictionary {
    if text.trim().is_empty() {
        return VarDictionary::new();
    }
    Json::parse_string(&GString::from(text))
        .try_to::<VarDictionary>()
        .unwrap_or_default()
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
