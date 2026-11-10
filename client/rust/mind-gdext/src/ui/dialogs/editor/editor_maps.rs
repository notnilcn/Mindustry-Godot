// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The editor maps dialog controller.
//! Upstream: ui/dialogs/EditorMapsDialog.java

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::batch_campaign2::{
    MapListState, install_frame_tick, maplist_add_pane_button, maplist_build,
    maplist_load_settings, maplist_maps, maplist_rebuild, maplist_refresh_registry,
};
use crate::ui::dialogs::rules::util::*;
use crate::ui::kit;
use godot::builtin::{VarArray, VarDictionary};
use godot::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "editor_maps" => Some(Box::new(EditorMapsDialog::default())),
        _ => None,
    }
}

/// `EditorMapsDialog`/`ui.editor.show()` default size for New Map.
const NEW_MAP_SIZE: i32 = 200;
/// `_batch_d_maps_rebuild`: set by the `editor_map` page after a delete/import
/// (`batch_d::editor_maps_request_rebuild`).
const MAPS_REBUILD_META: &str = "_batch_d_maps_rebuild";

/// `EditorMapsDialog` state: the shared `MapListState` plus the prompt flags
/// (`_chooser_action`, `_chooser_connected`, `_pending_new_map`,
/// `_pending_import`).
struct EditorMapsState {
    list: Rc<RefCell<MapListState>>,
    chooser_action: String,
    chooser_connected: bool,
    pending_new_map: bool,
    pending_import: String,
}

impl Default for EditorMapsState {
    fn default() -> Self {
        Self {
            list: Rc::new(RefCell::new(MapListState::new())),
            chooser_action: String::new(),
            chooser_connected: false,
            pending_new_map: false,
            pending_import: String::new(),
        }
    }
}

/// `@maps` — the editor map browser (`EditorMapsDialog extends MapListDialog`).
#[derive(Default)]
pub struct EditorMapsDialog {
    state: Rc<RefCell<EditorMapsState>>,
}

impl DialogController for EditorMapsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@maps");
        dialog.should_pause = false;
        dialog.enable_full_dialog(true);
        // `makeButtonOverlay()` (`MapListDialog.java:103`).
        dialog.apply_button_overlay(true);
        let list = self.state.borrow().list.clone();
        // `displayType = true` (`EditorMapsDialog.java:17`).
        list.borrow_mut().display_type = true;
        // `showMap(map)` override (`EditorMapsDialog._show_map`).
        list.borrow_mut().show_map =
            Some(Rc::new(|dialog: &mut MindDialog, map: &VarDictionary| {
                let mut context = VarDictionary::new();
                context.set(
                    "map",
                    &GString::from(dict_text(map, "name").as_str()).to_variant(),
                );
                dialog.open_dialog("editor_map", &json_stringify(&context));
            }));
        maplist_load_settings(dialog, &list);
        maplist_build(dialog, &list);
        // `buildButtons`: `@editor.newmap` (`Icon.add`) then `@editor.importmap`
        // (`Icon.upload`), both 210x64, after the shared `@back`.
        let target = dialog.to_gd();
        let state_for_new = self.state.clone();
        let new_map = {
            let target = target.clone();
            Callable::from_fn("editor_maps_new_map", move |_args: &[&Variant]| {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                new_map_prompt(&mut bound, &state_for_new);
            })
        };
        maplist_add_pane_button(dialog, "@editor.newmap", "add", Some(new_map));
        let state_for_import = self.state.clone();
        let import = {
            let target = target.clone();
            Callable::from_fn("editor_maps_import", move |_args: &[&Variant]| {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                open_import(&mut bound, &state_for_import);
            })
        };
        maplist_add_pane_button(dialog, "@editor.importmap", "upload", Some(import));
        connect_prompts(dialog, &self.state);
        let deferred = self.state.borrow().list.borrow().deferred.clone();
        install_frame_tick(dialog, "editor_maps", deferred, || {});
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let list = self.state.borrow().list.clone();
        maplist_load_settings(dialog, &list);
        maplist_refresh_registry(dialog, &list);
        // `setup`: `searchString = null` and a fresh (empty) search field.
        list.borrow_mut().search = String::new();
        let field = list.borrow().field.clone();
        if let Some(mut field) = field {
            field.set_text("");
        }
        maplist_rebuild(dialog, &list);
        // Desktop focuses the search field (`MapListDialog.java:367-369`).
        if let Some(mut field) = list.borrow().field.clone() {
            field.call_deferred("grab_focus", &[]);
        }
        // The `editor_map` page asked for a rebuild (delete/import).
        if dialog.base().has_meta(MAPS_REBUILD_META) {
            dialog.base_mut().remove_meta(MAPS_REBUILD_META);
        }
    }
}

/// `buildButtons`: `ui.showTextInput("@editor.newmap", "@editor.mapname", "", ...)`.
fn new_map_prompt(_dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("show_text_input") {
        return;
    }
    state.borrow_mut().pending_new_map = true;
    let _ = ui.call(
        "show_text_input",
        &[
            kit::t("@editor.newmap").to_variant(),
            kit::t("@editor.mapname").to_variant(),
            50i64.to_variant(),
            GString::new().to_variant(),
            false.to_variant(),
            false.to_variant(),
        ],
    );
}

/// `buildButtons`: `@editor.importmap` -> `FileChooser.open(mapExtension)`.
fn open_import(dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>) {
    let Some(ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("open_dialog") {
        return;
    }
    state.borrow_mut().chooser_action = String::from("import");
    connect_chooser(dialog, state, true);
    let mut context = VarDictionary::new();
    context.set("open", &true.to_variant());
    let mut extensions = VarArray::new();
    extensions.push(&GString::from("msav").to_variant());
    context.set("extensions", &extensions.to_variant());
    dialog.open_dialog("file_chooser", &json_stringify(&context));
}

/// `_connect_chooser(true/false)`: one `file_chosen` listener at a time.
fn connect_chooser(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<EditorMapsState>>,
    should_connect: bool,
) {
    let Some(mut chooser) = dialog_node("file_chooser") else {
        return;
    };
    if !chooser.has_signal("file_chosen") {
        return;
    }
    let mut node = dialog.to_gd();
    if !should_connect {
        if node.has_meta("_batch_i_chooser") {
            if let Ok(callable) = node.get_meta("_batch_i_chooser").try_to::<Callable>()
                && chooser.is_connected("file_chosen", &callable)
            {
                chooser.disconnect("file_chosen", &callable);
            }
            node.remove_meta("_batch_i_chooser");
        }
        state.borrow_mut().chooser_connected = false;
        return;
    }
    if state.borrow().chooser_connected {
        return;
    }
    let target = dialog.to_gd();
    let state_for = state.clone();
    let callable = Callable::from_fn("editor_maps_file_chosen", move |args: &[&Variant]| {
        let path = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        file_chosen(&mut bound, &state_for, &path);
    });
    chooser.connect("file_chosen", &callable);
    node.set_meta("_batch_i_chooser", &callable.to_variant());
    state.borrow_mut().chooser_connected = true;
}

/// `_on_file_chosen`: drop the route and start the import.
fn file_chosen(dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>, path: &str) {
    if state.borrow().chooser_action.is_empty() {
        return;
    }
    state.borrow_mut().chooser_action.clear();
    connect_chooser(dialog, state, false);
    let _ = ui_call(
        "close_dialog",
        &[GString::from("file_chooser").to_variant()],
    );
    state.borrow_mut().pending_import = path.to_string();
    import_map(dialog, state, false);
}

/// `EditorMapsDialog.tryImportMap` (`:55-98`): image -> `@editor.errorimage`;
/// a built-in name -> `editor.import.exists`; a custom conflict -> the
/// `editor.overwrite.confirm` confirm, then re-import with `overwrite`.
fn import_map(dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>, overwrite: bool) {
    let pending = state.borrow().pending_import.clone();
    let Some(mut editor) = kit::editor() else {
        state.borrow_mut().pending_import.clear();
        return;
    };
    let result = editor
        .call(
            "import_map",
            &[
                GString::from(pending.as_str()).to_variant(),
                overwrite.to_variant(),
            ],
        )
        .to::<GString>()
        .to_string();
    if result.is_empty() {
        state.borrow_mut().pending_import.clear();
        let list = state.borrow().list.clone();
        maplist_refresh_registry(dialog, &list);
        maplist_rebuild(dialog, &list);
        return;
    }
    if let Some(name) = result.strip_prefix("overwrite:").map(str::to_string) {
        confirm_overwrite(state, &name);
        return;
    }
    state.borrow_mut().pending_import.clear();
    if let Some(name) = result.strip_prefix("exists:") {
        notify_key(&fmt_key("@editor.import.exists", &[name]));
    } else if result == "image" {
        notify_key("@editor.errorimage");
    } else {
        notify_key("@editor.errorload");
    }
}

/// `_confirm_overwrite`: `ui.showConfirm("@editor.overwrite.confirm", name)`.
fn confirm_overwrite(state: &Rc<RefCell<EditorMapsState>>, map_name: &str) {
    let _ = state;
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("show_confirm") {
        return;
    }
    let text = kit::markup_text(&fmt_key("@editor.overwrite.confirm", &[map_name]));
    let _ = ui.call("show_confirm", &[text.to_variant()]);
}

/// `_connect_prompts`: `text_input_result` and `confirm_result` handlers.
fn connect_prompts(dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>) {
    let target = dialog.to_gd();
    let state_for_text = state.clone();
    let text_callable = {
        let target = target.clone();
        Callable::from_fn("editor_maps_text_input", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            let target = target.clone();
            let state_for_text = state_for_text.clone();
            defer_action("editor_maps_new_map_deferred", move || {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                on_text_input(&mut bound, &state_for_text, &text);
            });
        })
    };
    connect_prompt_once(
        dialog,
        "maps_text_input",
        "text_input_result",
        &text_callable,
    );
    let state_for_confirm = state.clone();
    let confirm_callable = {
        let target = target.clone();
        Callable::from_fn("editor_maps_confirm", move |args: &[&Variant]| {
            let confirmed = args
                .first()
                .map(|value| (*value).to::<bool>())
                .unwrap_or(false);
            let target = target.clone();
            let state_for_confirm = state_for_confirm.clone();
            defer_action("editor_maps_confirm_deferred", move || {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                on_confirm(&mut bound, &state_for_confirm, confirmed);
            });
        })
    };
    connect_prompt_once(dialog, "maps_confirm", "confirm_result", &confirm_callable);
}

/// `_on_text_input`: a taken name shows `@editor.exists`, else a fresh 200x200
/// map with the `name` tag (`EditorMapsDialog.java:27-42`).
fn on_text_input(dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>, text: &str) {
    if !state.borrow().pending_new_map {
        return;
    }
    state.borrow_mut().pending_new_map = false;
    let map_name = text.trim().to_string();
    if map_name.is_empty() {
        return;
    }
    if row_for(dialog, &map_name).is_some() {
        notify_key("@editor.exists");
        return;
    }
    let Some(mut editor) = kit::editor() else {
        notify_key("@editor.errorload");
        return;
    };
    let started = editor
        .call(
            "begin_new",
            &[NEW_MAP_SIZE.to_variant(), NEW_MAP_SIZE.to_variant()],
        )
        .try_to::<bool>()
        .unwrap_or(false);
    if !started {
        notify_key("@editor.errorload");
        return;
    }
    let _ = editor.call(
        "set_tag",
        &[
            GString::from("name").to_variant(),
            GString::from(map_name.as_str()).to_variant(),
        ],
    );
    dialog.close();
    if let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
        && shell.has_method("show_dialog")
    {
        let _ = shell.call("show_dialog", &[]);
    }
}

/// `_on_confirm`: the overwrite answer for a pending import.
fn on_confirm(dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>, confirmed: bool) {
    if state.borrow().pending_import.is_empty() {
        return;
    }
    if !confirmed {
        state.borrow_mut().pending_import.clear();
        return;
    }
    import_map(dialog, state, true);
}

/// `_row_for(mapName)`: the registry row with that display name.
fn row_for(dialog: &mut MindDialog, map_name: &str) -> Option<VarDictionary> {
    maplist_maps(dialog)
        .into_iter()
        .find(|row| dict_text(row, "name") == map_name)
}

/// `showMap`'s `@editor.openin`: load the map into the editor and show the shell.
///
/// The GDScript keeps this helper unused (`editor_map` owns the working route);
/// it is ported for parity with the source.
#[allow(dead_code)]
fn open_in_editor(path: &str) {
    if path.is_empty() {
        notify_key("@editor.errorload");
        return;
    }
    let Some(mut editor) = kit::editor() else {
        notify_key("@editor.errorload");
        return;
    };
    let opened = editor
        .call("begin_edit_map", &[GString::from(path).to_variant()])
        .try_to::<bool>()
        .unwrap_or(false);
    if !opened {
        notify_key("@editor.errorload");
        return;
    }
    let _ = ui_call("close_dialog", &[GString::from("editor_maps").to_variant()]);
    if let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
        && shell.has_method("show_dialog")
    {
        let _ = shell.call("show_dialog", &[]);
    }
}
