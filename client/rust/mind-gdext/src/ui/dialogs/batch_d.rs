// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/EditorMapsDialog.java (@editor_maps,
//           @editor_map — the map browser and its per-map preview page),
//         core/src/mindustry/editor/MapEditorDialog.java (@editor_menu,
//           @editor_import, @editor_export),
//         core/src/mindustry/ui/dialogs/FileChooserDialog.java (@file_chooser),
//         core/src/mindustry/ui/dialogs/IconSelectDialog.java (@icon_select,
//           with `Vars.accessibleIcons`/`defaultContentIcons` and
//           PlanetDialog.java:1223-1283 for the sector marker mode),
//         core/src/mindustry/ui/dialogs/ColorPicker.java (@picker),
//         plan 14 §5 M1 (the dev-only widget gallery).

//! Editor dialogs, the fallback file chooser, the icon/colour pickers and the
//! dev widget gallery.
//!
//! Every controller mirrors one `client/ui/dialogs/*.gd` script and keeps its
//! data flow: the map registry and preview textures come from `MindPreview`,
//! editor state and every editor command go through `kit::editor()`, prompts
//! and the dialog stack through `MindUi`. Signals (`icon_selected`,
//! `sector_icon_selected`, `color_selected`, `file_chosen`) are declared on
//! [`MindDialog`] and emitted with `emit_signal`, so the `UiRoot` /
//! `EditorDialog` wiring keeps working unchanged.
//!
//! A `MindUi` prompt signal (`confirm_result`/`text_input_result`) is emitted
//! from inside a `MindUi` method, so the answering handler must not call back
//! into the facade synchronously (`Gd<T>::bind_mut` panics on a re-entrant
//! borrow, PAR-0066); those handlers queue their work through [`defer_action`].

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::object::ConnectFlags;
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::StretchMode;
use godot::classes::{
    Button, ColorRect, Control, DirAccess, HBoxContainer, Json, LineEdit, MarginContainer, Node,
    PanelContainer, RichTextLabel, ScrollContainer, StyleBoxFlat, Texture2D, VBoxContainer,
};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::bar::MindBar;
use super::super::kit::widgets::border_image::MindBorderImage;
use super::super::kit::widgets::check::MindCheck;
use super::super::kit::widgets::core_items_display::CoreItemsDisplay;
use super::super::kit::widgets::factory;
use super::super::kit::widgets::grid_image::MindGridImage;
use super::super::kit::widgets::items_display::ItemsDisplay;
use super::super::kit::widgets::warning_bar::MindWarningBar;
use super::super::kit::{self, MindLabel, MindScroll, MindTable, styles};

/// The controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "widget_gallery" => Some(Box::new(WidgetGalleryDialog)),
        "editor_import" => Some(Box::new(EditorImportDialog)),
        "editor_export" => Some(Box::new(EditorExportDialog)),
        "file_chooser" => Some(Box::new(FileChooserDialog::default())),
        "icon_select" => Some(Box::new(IconSelectDialog::default())),
        "editor_menu" => Some(Box::new(EditorMenuDialog)),
        "editor_map" => Some(Box::new(EditorMapDialog::default())),
        _ => None,
    }
}

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
fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

/// Calls a `MindUi` method (`None` when the autoload/method is absent).
fn ui_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut ui = kit::ui()?;
    if !ui.has_method(method) {
        return None;
    }
    Some(ui.call(method, args))
}

/// `MindUi.open_dialog(name, ctx)`.
fn open_named_dialog(name: &str, context_json: &str) {
    let _ = ui_call(
        "open_dialog",
        &[
            GString::from(name).to_variant(),
            GString::from(context_json).to_variant(),
        ],
    );
}

/// `MindUi.close_dialog(name)`.
fn close_named_dialog(name: &str) {
    let _ = ui_call("close_dialog", &[GString::from(name).to_variant()]);
}

/// A dialog node by manifest name (the `UiRoot` registry, else the tree).
fn dialog_node(name: &str) -> Option<Gd<Node>> {
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
fn notify_info(text: &str) {
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
fn notify_error(text: &str) {
    let title = kit::markup_text(&kit::t("@error.title").to_string());
    let body = kit::markup_text(text);
    let _ = ui_call("show_text", &[title.to_variant(), body.to_variant()]);
}

/// `ui.showErrorMessage(_t(key))`.
fn notify_error_key(key: &str) {
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
fn fmt_markup(key: &str, args: &[&str]) -> GString {
    kit::markup_text(&fmt(key, args))
}

/// A dictionary value coerced to text (`str(value)`; empty when absent).
fn dict_string(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| value.to::<GString>().to_string())
        .unwrap_or_default()
}

/// A dictionary value coerced to a bool.
fn dict_bool(dict: &VarDictionary, key: &str, default: bool) -> bool {
    dict.get(key)
        .map(|value| value.to::<bool>())
        .unwrap_or(default)
}

/// `JSON.stringify(dictionary)`.
fn json_stringify(value: &VarDictionary) -> String {
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
fn take_meta_string(node: &mut Gd<MindDialog>, key: &str) -> String {
    let name = StringName::from(key);
    if !node.has_meta(&name) {
        return String::new();
    }
    let value = node.get_meta(&name).to::<GString>().to_string();
    node.remove_meta(&name);
    value
}

/// Reads and clears one bool meta key (`false` when unset).
fn take_meta_bool(node: &mut Gd<MindDialog>, key: &str) -> bool {
    let name = StringName::from(key);
    if !node.has_meta(&name) {
        return false;
    }
    let value = node.get_meta(&name).to::<bool>();
    node.remove_meta(&name);
    value
}

/// Reads one string meta key (`""` when unset).
fn meta_string(node: &Gd<MindDialog>, key: &str) -> String {
    let name = StringName::from(key);
    if !node.has_meta(&name) {
        return String::new();
    }
    node.get_meta(&name).to::<GString>().to_string()
}

/// `BaseDialog.close()` from an engine callback.
fn close_node(target: &Gd<MindDialog>) {
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
const CHOOSER_ACTION_META: &str = "_batch_d_chooser_action";
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
fn chooser_context(is_open: bool, extensions: &[&str]) -> String {
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
fn open_chooser_route(
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
fn file_extension(file_name: &str) -> String {
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
fn path_join(base: &str, file: &str) -> String {
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
fn simplify_path(path: &str) -> String {
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

/// `Strings.stripColors`: drops `[]`, `[#rrggbb]` and colour-name tags.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn strip_colors(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let tag = &after[..end];
        let hex = tag.strip_prefix('#').is_some_and(|digits| {
            (3..=8).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_hexdigit())
        });
        let named = tag.chars().all(|c| c.is_ascii_lowercase());
        if !(hex || named) {
            out.push('[');
            out.push_str(tag);
            out.push(']');
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// WidgetGalleryDialog
// ---------------------------------------------------------------------------

/// Dev-only gallery: one of each M1 widget for in-engine/MCP inspection
/// (`client/ui/dialogs/widget_gallery.gd`).
pub struct WidgetGalleryDialog;

impl DialogController for WidgetGalleryDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        // `title_text = "Widget Gallery"` (a literal, not a bundle key).
        dialog.set_title_literal("Widget Gallery");
        self.build(dialog);
    }
}

impl WidgetGalleryDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        let Some(mut content) = dialog.content() else {
            return;
        };
        let mut grid = MindTable::new_alloc();
        grid.bind_mut().grow_horizontal();
        let control: Gd<Control> = grid.clone().upcast();
        content.add_child(&control);

        // code-instantiated: the gallery is a dev harness that adds one of each
        // widget; its contents are intentionally data/code-driven (plan 14 §5 M1).
        let mut bar = MindBar::new_alloc();
        bar.bind_mut()
            .setup(GString::from("Bar"), styles::ACCENT, 0.5);
        grid.bind_mut().add(&bar).width(200.0).height(24.0).pad(2.0);
        let warning = MindWarningBar::new_alloc();
        grid.bind_mut()
            .add(&warning)
            .width(200.0)
            .height(24.0)
            .pad(2.0);
        grid.bind_mut().row();

        let grid_image = MindGridImage::with_size(8, 4);
        grid.bind_mut()
            .add(&grid_image)
            .width(200.0)
            .height(64.0)
            .pad(2.0);
        let mut check = MindCheck::new_alloc();
        let noop = Callable::from_fn("gallery_check", |_args: &[&Variant]| {});
        check.bind_mut().setup(GString::from("Check"), true, noop);
        grid.bind_mut().add(&check).pad(2.0);
        grid.bind_mut().row();

        let items = ItemsDisplay::new_alloc();
        grid.bind_mut().add(&items).pad(2.0);
        let core_items = CoreItemsDisplay::new_alloc();
        grid.bind_mut().add(&core_items).pad(2.0);
        grid.bind_mut().row();

        let label = factory::label("Label");
        grid.bind_mut().add(&label).pad(2.0);
        let button = factory::button("Button");
        grid.bind_mut().add(&button).pad(2.0);

        dialog.add_close_button(210.0);
    }
}

// ---------------------------------------------------------------------------
// EditorImportDialog
// ---------------------------------------------------------------------------

/// `@editor.import`: the three import routes — the in-game map list
/// (`map_load`), a `.msav` file and a PNG (`client/ui/dialogs/
/// editor_import_dialog.gd`, `MapEditorDialog.createDialog("@editor.import")`).
pub struct EditorImportDialog;

/// One `createDialog` entry is a 360x90 left-aligned button.
const IMPORT_BUTTON_SIZE: Vector2 = Vector2::new(360.0, 90.0);

impl DialogController for EditorImportDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.import");
        dialog.should_pause = false;
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.build(dialog);
    }
}

impl EditorImportDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        // A reused dialog node: drop the previous open's body before rebuilding.
        dialog.clear_content();
        let mut root = dialog.content_table();
        let target = dialog.to_gd();

        // `loadDialog.show()`: the in-game editor map list.
        let open_map_load = {
            let target = target.clone();
            Callable::from_fn("editor_import_map_load", move |_args: &[&Variant]| {
                open_named_dialog("map_load", "");
                close_node(&target);
            })
        };
        add_chooser_entry(
            &mut root,
            "@editor.importmap",
            "@editor.importmap.description",
            "download",
            open_map_load,
        );

        // `FileChooser.open(extension)` -> `beginEdit(Map|Pixmap)`.
        let chooser = chooser_context(true, &["msav"]);
        let open_file = {
            let target = target.clone();
            Callable::from_fn("editor_import_file", move |_args: &[&Variant]| {
                open_chooser_route(&target, "importfile", &chooser, |dialog, path| {
                    editor_import_file_chosen(dialog, path);
                });
            })
        };
        add_chooser_entry(
            &mut root,
            "@editor.importfile",
            "@editor.importfile.description",
            "file-text",
            open_file,
        );

        let chooser = chooser_context(true, &["png"]);
        let open_image = {
            let target = target.clone();
            Callable::from_fn("editor_import_image", move |_args: &[&Variant]| {
                open_chooser_route(&target, "importimage", &chooser, |dialog, path| {
                    editor_import_file_chosen(dialog, path);
                });
            })
        };
        add_chooser_entry(
            &mut root,
            "@editor.importimage",
            "@editor.importimage.description",
            "image",
            open_image,
        );
    }
}

/// One `createDialog` row: icon + wrapped name + grey description.
fn add_chooser_entry(
    root: &mut Gd<MindTable>,
    name_key: &str,
    desc_key: &str,
    icon: &str,
    handler: Callable,
) {
    // code-instantiated: the import/export entries are data-driven (fixed rows).
    let mut button = factory::button("");
    button.set_custom_minimum_size(IMPORT_BUTTON_SIZE);
    button.connect("pressed", &handler);
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.add_theme_constant_override("separation", 10);
    row.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&row);
    let mut glyph = factory::glyph_label(icon, 26, Color::WHITE);
    glyph.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    glyph.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&glyph);
    let mut text = VBoxContainer::new_alloc();
    text.set_h_size_flags(SizeFlags::EXPAND_FILL);
    text.set_alignment(AlignmentMode::CENTER);
    text.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&text);
    let mut name_label = factory::label(&kit::t(name_key).to_string());
    name_label.set_mouse_filter(MouseFilter::IGNORE);
    text.add_child(&name_label);
    let mut description = factory::label(&kit::t(desc_key).to_string());
    let mut description_rich = rich(&description);
    description_rich.add_theme_color_override("default_color", styles::ARC_GRAY);
    description_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
    description.set_mouse_filter(MouseFilter::IGNORE);
    text.add_child(&description);
    root.bind_mut()
        .add(&button)
        .width(IMPORT_BUTTON_SIZE.x)
        .height(IMPORT_BUTTON_SIZE.y)
        .pad(2.0);
    root.bind_mut().row();
}

/// `_on_file_chosen`: `beginEdit(Map|Pixmap)` for the pending route.
fn editor_import_file_chosen(target: &mut Gd<MindDialog>, path: &str) {
    let action = take_meta_string(target, CHOOSER_ACTION_META);
    close_named_dialog("file_chooser");
    let Some(mut editor) = kit::editor() else {
        return;
    };
    match action.as_str() {
        "importfile" => {
            // `MapIO.isImage(file)` -> `@editor.errorimage`.
            let extension = file_extension(path).to_lowercase();
            if extension == "png" || extension == "jpg" || extension == "jpeg" {
                notify_error_key("@editor.errorimage");
                return;
            }
            let opened = editor
                .call("begin_edit_map", &[GString::from(path).to_variant()])
                .try_to::<bool>()
                .unwrap_or(false);
            if opened {
                close_node(target);
            } else {
                notify_error_key("@editor.errorload");
            }
        }
        "importimage" => {
            let opened = editor
                .call("begin_edit_image", &[GString::from(path).to_variant()])
                .try_to::<bool>()
                .unwrap_or(false);
            if opened {
                close_node(target);
            } else {
                notify_error_key("@editor.errorload");
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// EditorExportDialog
// ---------------------------------------------------------------------------

/// `@editor.export`: the two export routes — the `.msav` (`MapIO.writeMap`) and
/// a PNG (`MapIO.writeImage`) (`client/ui/dialogs/editor_export_dialog.gd`).
pub struct EditorExportDialog;

impl DialogController for EditorExportDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.export");
        dialog.should_pause = false;
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.build(dialog);
    }
}

impl EditorExportDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        dialog.clear_content();
        let mut root = dialog.content_table();
        let target = dialog.to_gd();

        let chooser = chooser_context(false, &["msav"]);
        let export_file = {
            let target = target.clone();
            Callable::from_fn("editor_export_file", move |_args: &[&Variant]| {
                open_chooser_route(&target, "exportfile", &chooser, |dialog, path| {
                    editor_export_file_chosen(dialog, path);
                });
            })
        };
        add_chooser_entry(
            &mut root,
            "@editor.exportfile",
            "@editor.exportfile.description",
            "file-text",
            export_file,
        );

        let chooser = chooser_context(false, &["png"]);
        let export_image = {
            let target = target.clone();
            Callable::from_fn("editor_export_image", move |_args: &[&Variant]| {
                open_chooser_route(&target, "exportimage", &chooser, |dialog, path| {
                    editor_export_file_chosen(dialog, path);
                });
            })
        };
        add_chooser_entry(
            &mut root,
            "@editor.exportimage",
            "@editor.exportimage.description",
            "image",
            export_image,
        );
    }
}

/// `_on_file_chosen`: `MapIO.writeMap`/`writeImage` for the pending route.
fn editor_export_file_chosen(target: &mut Gd<MindDialog>, path: &str) {
    let action = take_meta_string(target, CHOOSER_ACTION_META);
    close_named_dialog("file_chooser");
    let Some(mut editor) = kit::editor() else {
        return;
    };
    let ok = match action.as_str() {
        "exportfile" => editor
            .call("export_map", &[GString::from(path).to_variant()])
            .try_to::<bool>()
            .unwrap_or(false),
        "exportimage" => editor
            .call("export_image", &[GString::from(path).to_variant()])
            .try_to::<bool>()
            .unwrap_or(false),
        _ => false,
    };
    if action.is_empty() {
        return;
    }
    if ok {
        close_node(target);
    } else {
        notify_error_key("@editor.errorsave");
    }
}

// ---------------------------------------------------------------------------
// FileChooserDialog
// ---------------------------------------------------------------------------

/// Fallback file browser (`@file_chooser`, `client/ui/dialogs/
/// file_chooser_dialog.gd`, `FileChooserDialog.java`): a path field over the
/// directory listing, an extension filter and the one-shot `file_chosen`
/// signal. The native `DisplayServer.file_dialog_show` path is plan 22's.
pub struct FileChooserDialog {
    state: Rc<RefCell<FileChooserState>>,
}

/// `_open`/`_extensions`/`_directory` plus the built field and list.
struct FileChooserState {
    is_open: bool,
    extensions: Vec<String>,
    directory: String,
    field: Option<Gd<LineEdit>>,
    list: Option<Gd<MindTable>>,
}

impl Default for FileChooserState {
    fn default() -> Self {
        Self {
            is_open: true,
            extensions: Vec::new(),
            directory: "user://".to_string(),
            field: None,
            list: None,
        }
    }
}

impl Default for FileChooserDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(FileChooserState::default())),
        }
    }
}

impl DialogController for FileChooserDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        dialog.set_title_key("@open");
        self.build(dialog);
        dialog.add_close_button(210.0);
        self.rebuild(dialog);
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        let is_open = dialog
            .context_value("open")
            .map(|value| value.to::<bool>())
            .unwrap_or(true);
        let extensions = string_array(dialog.context_value("extensions"));
        {
            let mut state = self.state.borrow_mut();
            state.is_open = is_open;
            if dialog.context_value("extensions").is_some() {
                state.extensions = extensions;
            }
        }
        dialog.set_title_key(if is_open { "@open" } else { "@save" });
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        // `FileChooserDialog.open` filters by its extensions; the GDScript only
        // rebuilt in `_ready`, so a re-open kept the previous listing (and the
        // new extension list never reached the filter).
        self.rebuild(dialog);
    }
}

impl FileChooserDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        let mut root = dialog.content_table();
        let mut field = factory::field(&kit::t("@filechooser.path").to_string());
        let target = dialog.to_gd();
        let state = self.state.clone();
        let callable = Callable::from_fn("file_chooser_navigate", move |args: &[&Variant]| {
            let path = args
                .first()
                .and_then(|value| value.try_to::<GString>().ok())
                .unwrap_or_default();
            file_chooser_navigate(&target, &state, &path.to_string());
        });
        field.connect("text_submitted", &callable);
        root.bind_mut().add(&field).grow_x().pad(4.0);
        root.bind_mut().row();
        let list = MindTable::new_alloc();
        root.bind_mut().add(&list).grow_x().grow_y();
        {
            let mut state = self.state.borrow_mut();
            state.field = Some(field);
            state.list = Some(list);
        }
    }

    fn rebuild(&mut self, dialog: &mut MindDialog) {
        let target = dialog.to_gd();
        file_chooser_rebuild(&target, &self.state);
    }
}

/// `_navigate`: sets the directory and re-lists it.
fn file_chooser_navigate(
    target: &Gd<MindDialog>,
    state: &Rc<RefCell<FileChooserState>>,
    path: &str,
) {
    {
        let mut state = state.borrow_mut();
        if !path.is_empty() {
            state.directory = path.to_string();
        }
    }
    file_chooser_rebuild(target, state);
}

/// `_rebuild`: `..`, the matching files, then the subdirectories.
fn file_chooser_rebuild(target: &Gd<MindDialog>, state: &Rc<RefCell<FileChooserState>>) {
    let (list, field, directory, extensions) = {
        let state = state.borrow();
        (
            state.list.clone(),
            state.field.clone(),
            state.directory.clone(),
            state.extensions.clone(),
        )
    };
    let (Some(mut list), Some(mut field)) = (list, field) else {
        return;
    };
    list.bind_mut().clear();
    field.set_text(&GString::from(directory.as_str()));
    let Some(dir) = DirAccess::open(&GString::from(directory.as_str())) else {
        // The GDScript handed the raw key to `label` (`@filechooser.unreadable`
        // was shown literally); resolve it here.
        let label = factory::label(&kit::t("@filechooser.unreadable").to_string());
        list.bind_mut().add(&label).pad(4.0);
        return;
    };
    // code-instantiated: directory entries are runtime filesystem data.
    if directory != "user://" && directory != "/" {
        let mut up = factory::button("..");
        let target_for_up = target.clone();
        let state_for_up = state.clone();
        let callable = Callable::from_fn("file_chooser_up", move |_args: &[&Variant]| {
            let current = state_for_up.borrow().directory.clone();
            let path = simplify_path(&path_join(&current, ".."));
            file_chooser_navigate(&target_for_up, &state_for_up, &path);
        });
        up.connect("pressed", &callable);
        list.bind_mut().add(&up).grow_x().pad(1.0);
        list.bind_mut().row();
    }
    for file_name in dir.get_files().as_slice() {
        let name = file_name.to_string();
        if !extensions.is_empty() && !file_chooser_matches(&name, &extensions) {
            continue;
        }
        let mut file = factory::button(&name);
        let target_for_file = target.clone();
        let state_for_file = state.clone();
        let callable = Callable::from_fn("file_chooser_pick", move |_args: &[&Variant]| {
            let current = state_for_file.borrow().directory.clone();
            let path = path_join(&current, &name);
            file_chooser_choose(&target_for_file, &path);
        });
        file.connect("pressed", &callable);
        list.bind_mut().add(&file).grow_x().pad(1.0);
        list.bind_mut().row();
    }
    for dir_name in dir.get_directories().as_slice() {
        let name = dir_name.to_string();
        let mut folder = factory::button(&format!("{name}/"));
        let target_for_folder = target.clone();
        let state_for_folder = state.clone();
        let callable = Callable::from_fn("file_chooser_enter", move |_args: &[&Variant]| {
            let current = state_for_folder.borrow().directory.clone();
            let path = path_join(&current, &name);
            file_chooser_navigate(&target_for_folder, &state_for_folder, &path);
        });
        folder.connect("pressed", &callable);
        list.bind_mut().add(&folder).grow_x().pad(1.0);
        list.bind_mut().row();
    }
}

/// `_matches`: the lowercased extension is one of the allowed ones.
fn file_chooser_matches(file_name: &str, extensions: &[String]) -> bool {
    let extension = file_extension(file_name).to_lowercase();
    extensions
        .iter()
        .any(|allowed| extension == allowed.to_lowercase())
}

/// `_choose`: emit `file_chosen(path)` and hide.
fn file_chooser_choose(target: &Gd<MindDialog>, path: &str) {
    if !target.is_instance_valid() {
        return;
    }
    emit_dialog_signal(target, "file_chosen", &[GString::from(path).to_variant()]);
    hide_node(target);
}

/// Emits one `MindDialog` signal from an engine callback.
fn emit_dialog_signal(target: &Gd<MindDialog>, signal: &str, args: &[Variant]) {
    if !target.is_instance_valid() {
        return;
    }
    let mut node = target.clone();
    let _ = node.emit_signal(signal, args);
}

/// `BaseDialog.hide_dialog()` from an engine callback.
fn hide_node(target: &Gd<MindDialog>) {
    if !target.is_instance_valid() {
        return;
    }
    let mut node = target.clone();
    let _ = node.call("hide_dialog", &[]);
}

/// A context/read-model value as a list of strings (`PackedStringArray` or
/// `Array`; empty when absent).
fn string_array(value: Option<Variant>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    if let Ok(array) = value.try_to::<VarArray>() {
        return array
            .iter_shared()
            .map(|entry| entry.to::<GString>().to_string())
            .collect();
    }
    value
        .try_to::<PackedStringArray>()
        .unwrap_or_default()
        .as_slice()
        .iter()
        .map(|entry| entry.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// IconSelectDialog
// ---------------------------------------------------------------------------

/// `Vars.accessibleIcons` — order and Java field names are the stored ABI
/// (`SectorInfo.icon` keeps the field name; the display maps through
/// `MindIcons`).
const ACCESSIBLE_ICONS: [&str; 31] = [
    "effect",
    "power",
    "logic",
    "units",
    "liquid",
    "production",
    "defense",
    "turret",
    "distribution",
    "crafting",
    "settings",
    "cancel",
    "zoom",
    "ok",
    "star",
    "home",
    "pencil",
    "up",
    "down",
    "left",
    "right",
    "hammer",
    "warning",
    "tree",
    "admin",
    "map",
    "modePvp",
    "terrain",
    "modeSurvival",
    "commandRally",
    "commandAttack",
];

/// `Vars.defaultContentIcons`.
const CONTENT_TYPES: [&str; 5] = ["item", "liquid", "block", "unit", "status"];

/// `min(20, Core.graphics.getWidth() / Scl.scl(52))` (1152 px viewport -> 20).
const ICON_COLUMNS: usize = 20;

/// `@icon_select` (`client/ui/dialogs/icon_select_dialog.gd`,
/// `IconSelectDialog.java`): the vanilla `accessibleIcons` grid plus content
/// icons, and the sector marker mode (`PlanetDialog.java:1223-1283`).
pub struct IconSelectDialog {
    mode: String,
    checked_icon: String,
    checked_content: String,
    grid: Option<Gd<MindTable>>,
    /// `database_json` entries grouped by content type (content order per type).
    rows: [Vec<VarDictionary>; 5],
}

impl Default for IconSelectDialog {
    fn default() -> Self {
        Self {
            mode: "icon".to_string(),
            checked_icon: String::new(),
            checked_content: String::new(),
            grid: None,
            rows: Default::default(),
        }
    }
}

impl DialogController for IconSelectDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.mode = {
            let mode = dialog.context_string("mode");
            if mode.is_empty() {
                "icon".to_string()
            } else {
                mode.to_string()
            }
        };
        self.checked_icon = dialog.context_string("icon").to_string();
        self.checked_content = dialog.context_string("content_icon").to_string();
        self.load_rows(dialog);
        self.rebuild(dialog);
    }
}

impl IconSelectDialog {
    /// `MindCampaign.database_json` entries grouped by type: hidden content is
    /// already dropped by the model; locked entries are filtered at build time
    /// like `u.unlocked()` (`PlanetDialog.java:1263`).
    fn load_rows(&mut self, dialog: &mut MindDialog) {
        self.rows = Default::default();
        let view = dialog.campaign_json("database_json", &[]);
        for category in array_of(&view, "categories").iter_shared() {
            let Ok(category) = category.try_to::<VarDictionary>() else {
                continue;
            };
            for tag in array_of(&category, "tags").iter_shared() {
                let Ok(tag) = tag.try_to::<VarDictionary>() else {
                    continue;
                };
                for entry in array_of(&tag, "entries").iter_shared() {
                    let Ok(entry) = entry.try_to::<VarDictionary>() else {
                        continue;
                    };
                    let type_name = dict_string(&entry, "content_type");
                    let Some(index) = CONTENT_TYPES.iter().position(|kind| *kind == type_name)
                    else {
                        continue;
                    };
                    self.rows[index].push(entry);
                }
            }
        }
    }

    /// `IconSelectDialog` grid: 48 px buttons, `cols = min(20, width/52)`, the
    /// none button, the accessible icons, then each `defaultContentIcons` type
    /// behind a 3 px accent rule. Java renders it in a `cont.pane`, so it
    /// scrolls.
    fn rebuild(&mut self, dialog: &mut MindDialog) {
        if self.grid.is_none() {
            let mut table = dialog.content_table();
            // code-instantiated: the icon grid is a scrolling pane sized by
            // content rows (`IconSelectDialog.java:26-31` `cont.pane`).
            let mut scroll = MindScroll::new_alloc();
            scroll.set_custom_minimum_size(Vector2::new(1000.0, 420.0));
            scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
            let mut grid = MindTable::new_alloc();
            grid.bind_mut().grow_horizontal();
            let grid_control: Gd<Control> = grid.clone().upcast();
            scroll.add_child(&grid_control);
            table.bind_mut().add(&scroll).grow_x().grow_y().pad(4.0);
            table.bind_mut().row();
            self.grid = Some(grid);
        }
        let Some(mut grid) = self.grid.clone() else {
            return;
        };
        grid.bind_mut().clear();
        let sector_mode = self.mode == "sector";
        let none_checked = self.checked_icon.is_empty() && self.checked_content.is_empty();
        let target = dialog.to_gd();

        let none_control: Gd<Control> = factory::glyph_label("none", 28, Color::WHITE).upcast();
        let none_button = icon_toggle(&none_control, none_checked, {
            let target = target.clone();
            Callable::from_fn("icon_select_none", move |_args: &[&Variant]| {
                if sector_mode {
                    icon_select_choose_sector(&target, true, "", "");
                } else {
                    icon_select_choose_icon(&target, false, "");
                }
            })
        });
        grid.bind_mut().add(&none_button).size(48.0);

        let mut index = 1usize;
        for icon_name in ACCESSIBLE_ICONS {
            let control: Gd<Control> = factory::glyph_label(icon_name, 28, Color::WHITE).upcast();
            let checked = icon_name == self.checked_icon;
            let button = icon_toggle(&control, checked, {
                let target = target.clone();
                Callable::from_fn("icon_select_icon", move |_args: &[&Variant]| {
                    icon_select_choose_icon(&target, sector_mode, icon_name);
                })
            });
            grid.bind_mut().add(&button).size(48.0);
            index += 1;
            if index.is_multiple_of(ICON_COLUMNS) {
                grid.bind_mut().row();
            }
        }

        for (type_index, type_name) in CONTENT_TYPES.iter().enumerate() {
            grid.bind_mut().row();
            let mut rule = ColorRect::new_alloc();
            rule.set_color(styles::ACCENT);
            rule.set_custom_minimum_size(Vector2::new(0.0, 3.0));
            rule.set_mouse_filter(MouseFilter::IGNORE);
            grid.bind_mut()
                .add(&rule)
                .grow_x()
                .colspan(ICON_COLUMNS as i32);
            grid.bind_mut().row();
            index = 0;
            for entry in self.rows[type_index].clone() {
                if !dict_bool(&entry, "unlocked", false) {
                    continue;
                }
                let entry_name = dict_string(&entry, "name");
                let checked = entry_name == self.checked_content;
                let button = icon_content_button(&target, &entry, type_name, &entry_name, checked);
                grid.bind_mut().add(&button).size(48.0);
                index += 1;
                if index.is_multiple_of(ICON_COLUMNS) {
                    grid.bind_mut().row();
                }
            }
        }
    }
}

/// One 48 px `squareTogglei` cell around the given content control
/// (`PlanetDialog.java:1236-1256`).
fn icon_toggle(content: &Gd<Control>, checked: bool, on_pressed: Callable) -> Gd<Button> {
    // code-instantiated: one data-driven toggle per accessible icon.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("squareTogglei");
    button.set_toggle_mode(true);
    button.set_pressed(checked);
    button.set_custom_minimum_size(Vector2::new(48.0, 48.0));
    let mut content = content.clone();
    content.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    content.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&content);
    button.connect("pressed", &on_pressed);
    button
}

/// One content entry: `uiIcon` via the `<type>-<name>-ui` / `-full` region
/// chain (`UnlockableContent.uiIcon`; `PlanetDialog.java:1265-1275`).
fn icon_content_button(
    target: &Gd<MindDialog>,
    entry: &VarDictionary,
    type_name: &str,
    entry_name: &str,
    checked: bool,
) -> Gd<Button> {
    let ui_region = format!("{type_name}-{entry_name}-ui");
    let full_region = format!("{type_name}-{entry_name}-full");
    // code-instantiated: one data-driven toggle per unlocked content entry.
    let mut button = factory::image_button_first(
        &[ui_region.as_str(), full_region.as_str(), entry_name],
        "squareTogglei",
    );
    button.set_toggle_mode(true);
    button.set_pressed(checked);
    button.set_custom_minimum_size(Vector2::new(48.0, 48.0));
    button.set_expand_icon(true);
    button.add_theme_constant_override("icon_max_width", 32);
    button.set_tooltip_text(&kit::t(&format!("@{type_name}.{entry_name}.name")).to_string());
    let _ = entry;
    let target = target.clone();
    let name = entry_name.to_string();
    let callable = Callable::from_fn("icon_select_content", move |_args: &[&Variant]| {
        // `_content_button` always routes through `_choose_sector`; outside
        // sector mode that falls back to `_choose_icon("")`.
        icon_select_choose_sector(&target, false, "", &name);
    });
    button.connect("pressed", &callable);
    button
}

/// Vanilla mode (schematics tags): the picked accessible icon name.
fn icon_select_choose_icon(target: &Gd<MindDialog>, sector_mode: bool, icon_name: &str) {
    if sector_mode {
        icon_select_choose_sector(target, true, icon_name, "");
        return;
    }
    if !target.is_instance_valid() {
        return;
    }
    emit_dialog_signal(
        target,
        "icon_selected",
        &[GString::from(icon_name).to_variant()],
    );
    hide_node(target);
}

/// Sector mode: the picked `Iconc` field name or content name; the unused half
/// is empty (`PlanetDialog.java:1238-1281` nulls the other).
fn icon_select_choose_sector(
    target: &Gd<MindDialog>,
    sector_mode: bool,
    icon_name: &str,
    content_name: &str,
) {
    if !sector_mode {
        if icon_name.is_empty() {
            icon_select_choose_icon(target, false, icon_name);
        }
        return;
    }
    if !target.is_instance_valid() {
        return;
    }
    emit_dialog_signal(
        target,
        "sector_icon_selected",
        &[
            GString::from(icon_name).to_variant(),
            GString::from(content_name).to_variant(),
        ],
    );
}

/// A read-model array field (`[]` when absent or another shape).
fn array_of(dict: &VarDictionary, key: &str) -> VarArray {
    dict.get(key)
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// EditorMenuDialog
// ---------------------------------------------------------------------------

/// One `MapEditorDialog` menu row (`ENTRIES`); `wide` entries span both
/// columns and carry the `menu.cont` cell padding.
struct MenuEntry {
    key: &'static str,
    icon: &'static str,
    action: &'static str,
    wide: bool,
    pad_bottom: f32,
}

/// The editor menu grid rows in order (the Steam workshop button is skipped).
const MENU_ENTRIES: [MenuEntry; 10] = [
    MenuEntry {
        key: "@editor.savemap",
        icon: "save",
        action: "save",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.mapinfo",
        icon: "pencil",
        action: "map_info",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.generate",
        icon: "terrain",
        action: "map_generate",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.resize",
        icon: "resize",
        action: "map_resize",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.import",
        icon: "download",
        action: "editor_import",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.export",
        icon: "upload",
        action: "editor_export",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.ingame",
        icon: "right",
        action: "ingame",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.playtest",
        icon: "play",
        action: "playtest",
        wide: false,
        pad_bottom: 0.0,
    },
    // `MapEditorDialog.java:181-184` (`menu.cont` cells; measured in
    // java/32-edmenu.png at y=370 and y=434).
    MenuEntry {
        key: "@editor.sectorgenerate",
        icon: "terrain",
        action: "sector_generate",
        wide: true,
        pad_bottom: 4.0,
    },
    MenuEntry {
        key: "@quit",
        icon: "exit",
        action: "quit",
        wide: true,
        pad_bottom: 0.0,
    },
];

/// `MapEditorDialog` button sizes: `swidth = 180f`, buttons `(swidth, 60)`,
/// wide rows `swidth * 2 + 10`.
const MENU_BUTTON_SIZE: Vector2 = Vector2::new(180.0, 60.0);
const MENU_WIDE_SIZE: Vector2 = Vector2::new(370.0, 60.0);

/// `@editor_menu` (`client/ui/dialogs/editor_menu_dialog.gd`,
/// `MapEditorDialog.java:65-270`): the editor menu grid. Every action forwards
/// to `/root/Spine/MindEditor` or opens another editor dialog; the shell owns
/// Escape and the unsaved-changes confirm.
pub struct EditorMenuDialog;

impl DialogController for EditorMenuDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@menu");
        dialog.should_pause = false;
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.build(dialog);
    }
}

impl EditorMenuDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        // A reused dialog node: drop the previous open's body before rebuilding
        // (`shown()` rebuilds on every open).
        dialog.clear_content();
        let mut root = dialog.content_table();
        let target = dialog.to_gd();
        // code-instantiated: the menu grid is data-driven from `MENU_ENTRIES`.
        let mut grid = MindTable::new_alloc();
        let mut column = 0usize;
        for entry in MENU_ENTRIES {
            // code-instantiated: one button per `MapEditorDialog` menu entry.
            let mut button = factory::icon_button(
                entry.icon,
                &kit::t(entry.key).to_string(),
                "defaultt",
                AlignmentMode::BEGIN,
            );
            let action = entry.action;
            let callable = {
                let target = target.clone();
                Callable::from_fn("editor_menu_run", move |_args: &[&Variant]| {
                    editor_menu_run(&target, action);
                })
            };
            button.connect("pressed", &callable);
            let size = if entry.wide {
                MENU_WIDE_SIZE
            } else {
                MENU_BUTTON_SIZE
            };
            if entry.wide {
                // `t.defaults().size(...)`: wide rows span both columns and
                // carry the `menu.cont` padBottom only.
                let mut guard = grid.bind_mut();
                guard
                    .add(&button)
                    .width(size.x)
                    .height(size.y)
                    .colspan(2)
                    .pad_bottom(entry.pad_bottom);
                drop(guard);
                grid.bind_mut().row();
                column = 0;
            } else {
                // `t.defaults().size(swidth, 60f).padBottom(5).padRight(5)
                // .padLeft(5)` (`MapEditorDialog.java:71`): a 65 px row pitch
                // and 190 px columns.
                let mut guard = grid.bind_mut();
                guard
                    .add(&button)
                    .width(size.x)
                    .height(size.y)
                    .pad_left(5.0)
                    .pad_right(5.0)
                    .pad_bottom(5.0);
                drop(guard);
                column += 1;
                if column.is_multiple_of(2) {
                    grid.bind_mut().row();
                }
            }
        }
        if !column.is_multiple_of(2) {
            grid.bind_mut().row();
        }
        // `menu.cont.table(t -> ...)` under `cont.defaults().pad(3)`.
        root.bind_mut().add(&grid).pad(3.0);
    }
}

/// `_run`: dispatches one menu row.
fn editor_menu_run(target: &Gd<MindDialog>, action: &str) {
    match action {
        "save" => editor_menu_save(target),
        "map_info" | "map_generate" | "map_resize" | "editor_import" | "editor_export"
        | "sector_generate" => editor_menu_open_dialog(target, action),
        "ingame" => editor_menu_edit_in_game(target),
        "playtest" => editor_menu_playtest(target),
        "quit" => editor_menu_quit(target),
        _ => {}
    }
}

/// `MapEditorDialog.save` (`:410-450`): name -> info dialog + `@editor.save.noname`;
/// builtin map -> `@editor.save.overwrite`; success -> `@editor.saved`. The menu
/// hides on every path.
fn editor_menu_save(target: &Gd<MindDialog>) {
    let Some(mut editor) = kit::editor() else {
        return;
    };
    let result = editor
        .call("save", &[])
        .try_to::<VarDictionary>()
        .unwrap_or_default();
    if dict_bool(&result, "ok", false) {
        notify_info(&kit::t("@editor.saved").to_string());
        close_node(target);
        return;
    }
    match dict_string(&result, "error").as_str() {
        "noname" => {
            editor_menu_open_dialog(target, "map_info");
            notify_info(&kit::t("@editor.save.noname").to_string());
        }
        "overwrite" => notify_info(&kit::t("@editor.save.overwrite").to_string()),
        _ => notify_info(&kit::t("@editor.errorsave").to_string()),
    }
    close_node(target);
}

/// Every non-action entry opens a registered editor dialog (`menu.hide()` +
/// `dialog.show()` upstream).
fn editor_menu_open_dialog(target: &Gd<MindDialog>, dialog_name: &str) {
    // From the menu the generate dialog applies to the editor (`applyToEditor`).
    if dialog_name == "map_generate"
        && let Some(mut generate) = dialog_node("map_generate")
    {
        generate.set("applied", &true.to_variant());
    }
    open_named_dialog(dialog_name, "");
    close_node(target);
}

/// `MapEditorDialog.editInGame` (`:326-372`): save first, then play the hidden
/// `editor` gamemode.
fn editor_menu_edit_in_game(target: &Gd<MindDialog>) {
    let Some(mut editor) = kit::editor() else {
        return;
    };
    let saved = editor
        .call("save", &[])
        .try_to::<VarDictionary>()
        .unwrap_or_default();
    if !dict_bool(&saved, "ok", false) {
        notify_error_key("@editor.errorsave");
        return;
    }
    let map_name = dict_string(&editor_tags(), "name");
    if map_name.is_empty() {
        notify_error_key("@editor.errorsave");
        return;
    }
    let Some(mut campaign) = kit::campaign() else {
        notify_error_key("@editor.errorsave");
        return;
    };
    let started = campaign.has_method("start_map_mode")
        && campaign
            .call(
                "start_map_mode",
                &[
                    GString::from(map_name.as_str()).to_variant(),
                    GString::from("editor").to_variant(),
                    true.to_variant(),
                    GString::new().to_variant(),
                ],
            )
            .try_to::<bool>()
            .unwrap_or(false);
    if !started {
        notify_error_key("@editor.errorsave");
        return;
    }
    close_node(target);
    hide_editor_shell();
    show_game();
}

/// `MapEditorDialog.playtest` (`:374-393`): save, then play immediately (shift)
/// or open the map-play dialog in playtesting mode.
fn editor_menu_playtest(target: &Gd<MindDialog>) {
    let Some(mut editor) = kit::editor() else {
        return;
    };
    let result = editor
        .call("playtest", &[])
        .try_to::<VarDictionary>()
        .unwrap_or_default();
    if !dict_bool(&result, "ok", false) {
        let error_key = editor_save_error_key(&dict_string(&result, "error"));
        notify_error(&kit::t(error_key).to_string());
        return;
    }
    close_node(target);
    if dict_bool(&result, "dialog", false) {
        let mut context = VarDictionary::new();
        context.set(
            "map",
            &GString::from(dict_string(&editor_tags(), "name").as_str()).to_variant(),
        );
        context.set("playtest", &true.to_variant());
        open_named_dialog("map_play", &json_stringify(&context));
        return;
    }
    // Play started directly (the Java shift path): hide the editor shell.
    hide_editor_shell();
    show_game();
}

/// The menu's `@quit` -> `tryExit()` (`MapEditorDialog.java:262-265`): the shell
/// owns the unsaved-changes confirm.
fn editor_menu_quit(target: &Gd<MindDialog>) {
    close_node(target);
    if let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
        && shell.has_method("_confirm_exit")
    {
        let _ = shell.call("_confirm_exit", &[]);
    }
}

/// `Save`/`playtest` error -> bundle key (`MapEditorDialog.save`, `:410-450`).
fn editor_save_error_key(error: &str) -> &'static str {
    match error {
        "noname" => "@editor.save.noname",
        "overwrite" => "@editor.save.overwrite",
        _ => "@editor.errorsave",
    }
}

/// `MindEditor.tags()` (`{name, author, description}`, `{}` when absent).
fn editor_tags() -> VarDictionary {
    let Some(mut editor) = kit::editor() else {
        return VarDictionary::new();
    };
    if !editor.has_method("tags") {
        return VarDictionary::new();
    }
    editor
        .call("tags", &[])
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// Hides the editor shell (`/root/Spine/Ui/EditorDialog`).
fn hide_editor_shell() {
    if let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
        && shell.has_method("hide_dialog")
    {
        let _ = shell.call("hide_dialog", &[]);
    }
}

/// `ui_root.set_menu_visible(false)`: the game view without the menu.
fn show_game() {
    if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
        && root.has_method("set_menu_visible")
    {
        let _ = root.call("set_menu_visible", &[false.to_variant()]);
    }
}

// ---------------------------------------------------------------------------
// confirm / text-input prompt wiring
// ---------------------------------------------------------------------------

/// The map whose info page is open (`_name`), read by the confirm handler.
const EDITOR_MAP_NAME_META: &str = "_batch_d_editor_map_name";
/// A `@delete` confirm is awaiting an answer (`_pending_delete`).
const EDITOR_MAP_DELETE_META: &str = "_batch_d_editor_map_delete";

/// Connects a one-shot-style `MindUi.confirm_result` handler once per dialog.
///
/// The answer is queued ([`defer_action`]) because `MindUi` emits the signal
/// from inside its own method; the handler re-binds the dialog afterwards.
fn connect_confirm_prompt(
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
fn editor_maps_request_rebuild() {
    let Some(mut maps) = dialog_node("editor_maps") else {
        return;
    };
    maps.set_meta(MAPS_REBUILD_META, &true.to_variant());
    let _ = maps.call_deferred("refresh_controller", &[]);
}

// ---------------------------------------------------------------------------
// EditorMapDialog
// ---------------------------------------------------------------------------

/// `showMap`: `Core.graphics.isPortrait() ? 160f : 300f` (the port hardcodes
/// the desktop size in the GDScript).
const MAP_PREVIEW_SIZE: f32 = 300.0;
/// `mind_core::maps::UNKNOWN`: the `Map.tag` fallback that `maps_list` puts in
/// a row whose tag is missing/blank.
const MAP_MISSING_TAG: &str = "unknown";
/// `t.margin(6)` around the info rows (`EditorMapsDialog.java:113`).
const MAP_INFO_MARGIN: i32 = 6;
/// `t.defaults().padTop(10)` between caption rows.
const MAP_INFO_ROW_PAD: f32 = 10.0;
/// `.padTop(2)` above each value.
const MAP_INFO_VALUE_PAD: f32 = 2.0;
/// Wrapped-line pitch: Arc's `Fonts.def` lines are 20 px apart, `MindLabel`'s
/// 19 px (java/17b-capital.png: description lines at y=263, 283, 303, ...).
const MAP_ARC_LINE_EXTRA: i32 = 1;
/// An Arc label's box is one more pixel than its line pitch.
const MAP_ARC_LABEL_TAIL: f32 = 1.0;
/// Arc's cap offset inside a label box minus `MindLabel`'s (5 px vs 3 px).
const MAP_ARC_CAP_LEAD: f32 = 2.0;
/// `table.button(...).fillX().height(54f)` (`EditorMapsDialog.java:149,163`).
const MAP_BUTTON_HEIGHT: f32 = 54.0;

/// `@editor_map` (`client/ui/dialogs/editor_map_dialog.gd`,
/// `EditorMapsDialog.showMap` `:100-163`): the map preview beside the
/// name/author/description column, then `@editor.openin` and `@delete`
/// (disabled for built-in maps).
#[derive(Default)]
pub struct EditorMapDialog {
    name: String,
}

impl DialogController for EditorMapDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.mapinfo");
        dialog.should_pause = false;
        dialog.add_close_button(210.0);
        connect_confirm_prompt(dialog, "editor_map", |target, confirmed| {
            editor_map_on_confirm(target, confirmed);
        });
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        self.name = dialog.context_string("map").to_string();
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        if self.name.is_empty() {
            self.name = dialog.context_string("map").to_string();
        }
        self.build(dialog);
    }
}

impl EditorMapDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        // Java builds a fresh dialog per `showMap` (`EditorMapsDialog.java:101`);
        // this node is reused, so drop the previous open's body first.
        dialog.clear_content();
        let mut root = dialog.content_table();
        let row = editor_map_row(&self.name);
        let name = self.name.clone();
        {
            let mut node = dialog.to_gd();
            node.set_meta(
                EDITOR_MAP_NAME_META,
                &GString::from(name.as_str()).to_variant(),
            );
        }

        // `EditorMapsDialog.showMap` (`:100-163`): a 2x2 grid — the preview and
        // the `Styles.black` info pane are each `mapsize` (300) squares, and the
        // two action buttons `fillX()` their own column.
        if let Some(texture) = map_preview_texture(&name) {
            // code-instantiated: the map preview texture comes from the registry.
            let mut image = MindBorderImage::new_alloc();
            image.set_texture(&texture);
            // `new BorderImage(map.safeTexture())`: the default `thickness = 4f`
            // (`BorderImage.java:16`, `EditorMapsDialog.java:108`) and the
            // default nearest filter (`map.safeTexture()`).
            image.bind_mut().thickness = 4.0;
            image.bind_mut().force_nearest = true;
            image.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
            root.bind_mut()
                .add(&image)
                .width(MAP_PREVIEW_SIZE)
                .height(MAP_PREVIEW_SIZE);
        } else {
            // Keep the grid's first column even without a preview texture.
            // code-instantiated: layout placeholder for the missing preview.
            let placeholder = Control::new_alloc();
            root.bind_mut()
                .add(&placeholder)
                .width(MAP_PREVIEW_SIZE)
                .height(MAP_PREVIEW_SIZE);
        }

        // `table.table(Styles.black, desc -> { desc.top(); ...
        // desc.add(new ScrollPane(t)).grow(); ... }).height(mapsize)
        // .width(mapsize)` (`EditorMapsDialog.java:110-134`): the rows scroll
        // inside the fixed 300x300 pane.
        // code-instantiated: the info pane's rows come from the map's tags.
        let mut info = PanelContainer::new_alloc();
        let mut black = StyleBoxFlat::new_gd();
        black.set_bg_color(Color::BLACK);
        info.add_theme_stylebox_override("panel", &black);
        info.set_clip_contents(true);
        // code-instantiated: `new ScrollPane(t)` (default pane style, no X scroll).
        let mut pane = ScrollContainer::new_alloc();
        pane.set_horizontal_scroll_mode(ScrollMode::DISABLED);
        pane.set_h_size_flags(SizeFlags::EXPAND_FILL);
        pane.set_v_size_flags(SizeFlags::EXPAND_FILL);
        info.add_child(&pane);
        // code-instantiated: `t.margin(6)`.
        let mut margin = MarginContainer::new_alloc();
        margin.set_h_size_flags(SizeFlags::EXPAND_FILL);
        for side in ["left", "top", "bottom"] {
            margin.add_theme_constant_override(
                &StringName::from(&format!("margin_{side}")),
                MAP_INFO_MARGIN,
            );
        }
        // Hand the right margin to the text box: Godot's line breaker counts the
        // space after a word, Arc's `GlyphLayout` does not.
        margin.add_theme_constant_override("margin_right", 0);
        pane.add_child(&margin);
        let mut info_layout = VBoxContainer::new_alloc();
        info_layout.add_theme_constant_override("separation", 0);
        info_layout.set_alignment(AlignmentMode::BEGIN);
        info_layout.set_h_size_flags(SizeFlags::EXPAND_FILL);
        margin.add_child(&info_layout);
        // Every Arc label draws its cap 2 px lower in its box than a `MindLabel`
        // (java/17b-capital.png: pane top 122, "Map Name:" cap 133), so the whole
        // column starts `MAP_ARC_CAP_LEAD` lower.
        add_info_gap(&mut info_layout, MAP_ARC_CAP_LEAD);
        add_info_row(&mut info_layout, "@editor.mapname", &name, 0.0);
        let mut author = map_tag(&row, "author");
        if author.is_empty() {
            // `showMap`: built-ins with no author tag show "Anuke"; custom maps
            // show `map.author()` = `Map.tag` -> `@unknown`.
            author = if dict_bool(&row, "custom", false) {
                kit::t("@unknown").to_string()
            } else {
                "Anuke".to_string()
            };
        }
        add_info_row(
            &mut info_layout,
            "@editor.author",
            &author,
            MAP_INFO_ROW_PAD,
        );
        // `if(!map.tags.get("description", "").isEmpty())`: no description row at
        // all for a map without one.
        let description = map_tag(&row, "description");
        if !description.is_empty() {
            add_info_row(
                &mut info_layout,
                "@editor.description",
                &description,
                MAP_INFO_ROW_PAD,
            );
        }
        root.bind_mut()
            .add(&info)
            .width(MAP_PREVIEW_SIZE)
            .height(MAP_PREVIEW_SIZE);
        root.bind_mut().row();

        // `table.button("@editor.openin", Icon.export, ...).fillX().height(54f)`.
        let open_path = dict_string(&row, "path");
        let mut open_button = factory::icon_button(
            "export",
            &kit::t("@editor.openin").to_string(),
            "defaultt",
            AlignmentMode::BEGIN,
        );
        fix_map_button_height(&open_button);
        let callable = {
            let target = dialog.to_gd();
            Callable::from_fn("editor_map_open", move |_args: &[&Variant]| {
                editor_map_open_in_editor(&target, &open_path);
            })
        };
        open_button.connect("pressed", &callable);
        root.bind_mut()
            .add(&open_button)
            .width(MAP_PREVIEW_SIZE)
            .height(MAP_BUTTON_HEIGHT);

        // `table.button("@delete", Icon.trash, ...).disabled(!map.custom)`.
        let mut delete_button = factory::icon_button(
            "trash",
            &kit::t("@delete").to_string(),
            "defaultt",
            AlignmentMode::BEGIN,
        );
        fix_map_button_height(&delete_button);
        delete_button.set_disabled(!dict_bool(&row, "custom", false));
        let callable = {
            let target = dialog.to_gd();
            Callable::from_fn("editor_map_delete", move |_args: &[&Variant]| {
                editor_map_confirm_delete(&target);
            })
        };
        delete_button.connect("pressed", &callable);
        root.bind_mut()
            .add(&delete_button)
            .width(MAP_PREVIEW_SIZE)
            .height(MAP_BUTTON_HEIGHT);
        root.bind_mut().row();
    }
}

/// `.height(54f)`: Arc's cell size wins over the icon row's pref height
/// (`MindWidgets.icon_button` grows the button to its 31 px glyph + 24 px
/// padding on `ready`; this handler runs after it).
fn fix_map_button_height(button: &Gd<Button>) {
    let mut button = button.clone();
    let mut min = button.get_custom_minimum_size();
    min.y = MAP_BUTTON_HEIGHT;
    button.set_custom_minimum_size(min);
    let button_for_ready = button.clone();
    let callable = Callable::from_fn("editor_map_button_height", move |_args: &[&Variant]| {
        let mut button = button_for_ready.clone();
        let mut min = button.get_custom_minimum_size();
        min.y = MAP_BUTTON_HEIGHT;
        button.set_custom_minimum_size(min);
    });
    button.connect("ready", &callable);
}

/// A row tag as the raw `map.tags.get(name, "")` (blank when missing).
fn map_tag(row: &VarDictionary, tag_name: &str) -> String {
    let value = dict_string(row, tag_name).trim().to_string();
    if value == MAP_MISSING_TAG {
        String::new()
    } else {
        value
    }
}

/// One caption + value pair: `t.add(key).padRight(10).color(Color.gray)` then
/// `t.add(value).growX().wrap().padTop(2)` (`EditorMapsDialog.java:121-133`).
fn add_info_row(parent: &mut Gd<VBoxContainer>, label_key: &str, value: &str, pad_top: f32) {
    add_info_gap(parent, pad_top);
    let label = factory::label(&kit::t(label_key).to_string());
    let mut label_rich = rich(&label);
    label_rich.add_theme_color_override("default_color", styles::ARC_GRAY);
    label_rich.add_theme_constant_override("line_separation", MAP_ARC_LINE_EXTRA);
    parent.add_child(&label);
    add_info_gap(parent, MAP_ARC_LABEL_TAIL + MAP_INFO_VALUE_PAD);
    let text = factory::label(value);
    let mut text_rich = rich(&text);
    text_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
    text_rich.add_theme_constant_override("line_separation", MAP_ARC_LINE_EXTRA);
    parent.add_child(&text);
    add_info_gap(parent, MAP_ARC_LABEL_TAIL);
}

/// A cell `padTop` between two runtime-built rows.
fn add_info_gap(parent: &mut Gd<VBoxContainer>, height: f32) {
    if height <= 0.0 {
        return;
    }
    // code-instantiated: a cell `padTop` between two runtime-built rows.
    let mut gap = Control::new_alloc();
    gap.set_custom_minimum_size(Vector2::new(0.0, height));
    gap.set_mouse_filter(MouseFilter::IGNORE);
    parent.add_child(&gap);
}

/// The registry row for a map name (`MindPreview.maps_list`).
fn editor_map_row(map_name: &str) -> VarDictionary {
    preview_maps_list()
        .into_iter()
        .find(|row| dict_string(row, "name") == map_name)
        .unwrap_or_default()
}

/// The live registry rows (`MindPreview.maps_list`), hidden entries dropped.
fn preview_maps_list() -> Vec<VarDictionary> {
    let Some(mut preview) = kit::node("/root/Spine/MindPreview") else {
        return Vec::new();
    };
    if !preview.has_method("maps_list") {
        return Vec::new();
    }
    preview
        .call("maps_list", &[])
        .try_to::<VarArray>()
        .unwrap_or_default()
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .filter(|row| !dict_string(row, "name").is_empty())
        .collect()
}

/// `map.safeTexture()` for a display name (`MindPreview.texture_for_name`).
fn map_preview_texture(map_name: &str) -> Option<Gd<Texture2D>> {
    let mut preview = kit::node("/root/Spine/MindPreview")?;
    if !preview.has_method("texture_for_name") {
        return None;
    }
    preview
        .call("texture_for_name", &[GString::from(map_name).to_variant()])
        .try_to::<Gd<Texture2D>>()
        .ok()
}

/// `showMap`'s `@editor.openin` handler (delegated by `editor_map`).
///
/// `Vars.ui.editor.beginEditMap(map.file); dialog.hide(); hide();`: the maps
/// dialog loads the file, hides itself and shows the editor shell. A missing
/// maps dialog is a no-op (the GDScript null check), so the per-map page just
/// closes.
fn editor_map_open_in_editor(target: &Gd<MindDialog>, path: &str) {
    if dialog_node("editor_maps").is_some() {
        editor_load_map_into_editor(path);
    }
    close_node(target);
}

/// `EditorMapsDialog._open_in_editor`: load the map and show the shell.
fn editor_load_map_into_editor(path: &str) {
    if path.is_empty() {
        notify_error_key("@editor.errorload");
        return;
    }
    let Some(mut editor) = kit::editor() else {
        notify_error_key("@editor.errorload");
        return;
    };
    let opened = editor
        .call("begin_edit_map", &[GString::from(path).to_variant()])
        .try_to::<bool>()
        .unwrap_or(false);
    if !opened {
        notify_error_key("@editor.errorload");
        return;
    }
    close_named_dialog("editor_maps");
    if let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
        && shell.has_method("show_dialog")
    {
        let _ = shell.call("show_dialog", &[]);
    }
}

/// `_confirm_delete`: `ui.showConfirm("@confirm", map.delete(map.name()), ...)`.
fn editor_map_confirm_delete(target: &Gd<MindDialog>) {
    let mut node = target.clone();
    node.set_meta(EDITOR_MAP_DELETE_META, &true.to_variant());
    let name = meta_string(&node, EDITOR_MAP_NAME_META);
    let text = fmt_markup("@map.delete", &[name.as_str()]);
    let _ = ui_call("show_confirm", &[text.to_variant()]);
}

/// `_on_confirm`: deletes the map, refreshes the previews and the map browser.
fn editor_map_on_confirm(target: &Gd<MindDialog>, confirmed: bool) {
    let mut node = target.clone();
    if !take_meta_bool(&mut node, EDITOR_MAP_DELETE_META) {
        return;
    }
    if !confirmed {
        return;
    }
    let name = meta_string(&node, EDITOR_MAP_NAME_META);
    let Some(mut editor) = kit::editor() else {
        notify_error_key("@editor.errorload");
        return;
    };
    let deleted = editor
        .call("delete_map", &[GString::from(name.as_str()).to_variant()])
        .try_to::<bool>()
        .unwrap_or(false);
    if !deleted {
        notify_error_key("@editor.errorload");
        return;
    }
    if let Some(mut preview) = kit::node("/root/Spine/MindPreview")
        && preview.has_method("refresh")
    {
        let _ = preview.call("refresh", &[]);
    }
    editor_maps_request_rebuild();
    close_node(target);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_extension_matches_godot() {
        assert_eq!(file_extension("user://maps/a.msav"), "msav");
        assert_eq!(file_extension("a.png"), "png");
        assert_eq!(file_extension("noext"), "");
        assert_eq!(file_extension("dir.d/file"), "");
    }

    #[test]
    fn path_join_keeps_schemes() {
        assert_eq!(path_join("user://", "a.msav"), "user://a.msav");
        assert_eq!(path_join("user://maps", "a.msav"), "user://maps/a.msav");
        assert_eq!(path_join("user://maps/", "a.msav"), "user://maps/a.msav");
        assert_eq!(path_join("", "a.msav"), "a.msav");
        assert_eq!(path_join("user://maps", "user://b"), "user://b");
    }

    #[test]
    fn simplify_path_collapses_segments() {
        assert_eq!(simplify_path("user://a/../"), "user://");
        assert_eq!(simplify_path("user://a/b/.."), "user://a");
        assert_eq!(simplify_path("/a/./b//c"), "/a/b/c");
        assert_eq!(simplify_path("user://"), "user://");
    }

    #[test]
    fn strip_colors_drops_color_tags_only() {
        assert_eq!(strip_colors("[accent]Hail"), "Hail");
        assert_eq!(strip_colors("[#ff0000]Red[]"), "Red");
        assert_eq!(strip_colors("a[NotATag]b"), "a[NotATag]b");
        assert_eq!(strip_colors("no tags"), "no tags");
    }
}
