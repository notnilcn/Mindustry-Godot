// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The schematic import/export sub-dialog helpers. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/SchematicsDialog.java (showImport/showExport)

use super::files::*;
use super::schematics_dialog::*;
use super::subdialogs::*;
use crate::gdutil::variant::to_text_godot;
use crate::ui::dialog::MindDialog;
use crate::ui::dialogs::campaign::util::*;
use crate::ui::kit;
use crate::ui::kit::widgets::factory;
use godot::builtin::VariantType;
use godot::classes::box_container::AlignmentMode;
use godot::classes::file_dialog::{Access, FileMode};
use godot::classes::{DisplayServer, FileDialog};
use godot::prelude::*;
use mind_core::game::schematics;
use mind_core::io::{FileSystem, NativeFs};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

// ---------------------------------------------------------------------------
// SchematicsDialog: import/export sub-dialogs (`showImport`/`showExport`)
// ---------------------------------------------------------------------------

/// `showImport`: the `@editor.import` panel (clipboard + `.msch` file).
pub(super) fn schematics_show_import(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let mut body = schematic_overlay_open(
        dialog,
        state,
        SchematicOverlay::Import,
        "@editor.import",
        360.0,
    );
    let clipboard = DisplayServer::singleton().clipboard_get().to_string();
    // `disabled(b -> clipboard == null || !startsWith(schematicBaseStart))`.
    let mut load_clipboard = factory::icon_button(
        "copy",
        &kit::t("@load.clipboard").to_string(),
        "flatt",
        AlignmentMode::BEGIN,
    );
    load_clipboard.set_custom_minimum_size(Vector2::new(280.0, 60.0));
    load_clipboard.set_disabled(!schematic_clipboard_importable(&clipboard));
    {
        let target = dialog.to_gd();
        let state_for = state.clone();
        let callable =
            Callable::from_fn("schematic_load_clipboard", move |_args: &[&Variant]| {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                schematics_import_clipboard(&mut bound, &state_for);
            });
        load_clipboard.connect("pressed", &callable);
    }
    body.bind_mut().add(&load_clipboard).left().pad(4.0);
    body.bind_mut().row();
    let mut import_file = factory::icon_button(
        "download",
        &kit::t("@import.file").to_string(),
        "flatt",
        AlignmentMode::BEGIN,
    );
    import_file.set_custom_minimum_size(Vector2::new(280.0, 60.0));
    {
        let target = dialog.to_gd();
        let state_for = state.clone();
        let callable = Callable::from_fn("schematic_import_file", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_pick_import_files(&mut bound, &state_for);
        });
        import_file.connect("pressed", &callable);
    }
    body.bind_mut().add(&import_file).left().pad(4.0);
    body.bind_mut().row();
    schematic_overlay_finish(&mut body);
}

/// `showExport(s)`: the `@editor.export` panel (clipboard + `.msch` file).
pub(super) fn schematics_show_export(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
) {
    let Some(row) = schematic_row(dialog, index) else {
        return;
    };
    let mut body = schematic_overlay_open(
        dialog,
        state,
        SchematicOverlay::Export,
        "@editor.export",
        360.0,
    );
    let mut copy_clipboard = factory::icon_button(
        "copy",
        &kit::t("@copy.clipboard").to_string(),
        "flatt",
        AlignmentMode::BEGIN,
    );
    copy_clipboard.set_custom_minimum_size(Vector2::new(280.0, 60.0));
    {
        let target = dialog.to_gd();
        let state_for = state.clone();
        let callable =
            Callable::from_fn("schematic_copy_clipboard", move |_args: &[&Variant]| {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                schematics_export_clipboard(&mut bound, &state_for, index);
            });
        copy_clipboard.connect("pressed", &callable);
    }
    body.bind_mut().add(&copy_clipboard).left().pad(4.0);
    body.bind_mut().row();
    let mut export_file = factory::icon_button(
        "export",
        &kit::t("@export.file").to_string(),
        "flatt",
        AlignmentMode::BEGIN,
    );
    export_file.set_custom_minimum_size(Vector2::new(280.0, 60.0));
    {
        let target = dialog.to_gd();
        let state_for = state.clone();
        let name = row.name.clone();
        let callable = Callable::from_fn("schematic_export_file", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_pick_export_file(&mut bound, &state_for, index, &name);
        });
        export_file.connect("pressed", &callable);
    }
    body.bind_mut().add(&export_file).left().pad(4.0);
    body.bind_mut().row();
    schematic_overlay_finish(&mut body);
}

/// `Schematics.readBase64(Core.app.getClipboardText())` + `add` + `showInfo`.
fn schematics_import_clipboard(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let text = DisplayServer::singleton().clipboard_get().to_string();
    if !schematic_clipboard_importable(&text) {
        let none = kit::t("@none").to_string();
        dialog.show_toast(&none);
        return;
    }
    let decoded =
        with_content_registry(|registry| schematics::read_base64(&text, registry).ok()).flatten();
    let Some(mut schematic) = decoded else {
        let text = kit::t("@error.title").to_string();
        dialog.show_toast(&text);
        return;
    };
    schematic.remove_steam_id();
    let dir = schematic_directory();
    let path = schematic_free_path(&dir, &schematic.name(), |candidate| {
        NativeFs.exists(candidate)
    });
    let encoded =
        with_content_registry(|registry| schematics::write(&schematic, registry).ok()).flatten();
    let Some(encoded) = encoded else {
        return;
    };
    if NativeFs.write(&path, &encoded).is_err() {
        return;
    }
    schematics_finish_import(dialog, state, &path);
}

/// `Schematics.read(file)` per selected `.msch` (`submitMulti`), then show the
/// last one (`showImport` file half).
fn schematics_import_files(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    files: &[String],
) {
    let mut last = None;
    for file in files {
        let path = Path::new(file);
        let Ok(bytes) = NativeFs.read(path) else {
            continue;
        };
        if let Some(imported) = schematic_import_bytes(&bytes) {
            last = Some(imported);
        }
    }
    if let Some(path) = last {
        schematics_finish_import(dialog, state, &path);
    } else {
        let none = kit::t("@none").to_string();
        dialog.show_toast(&none);
    }
}

/// `setup(); ui.showInfoFade("@schematic.saved"); checkTags(s); showInfo(s)`.
fn schematics_finish_import(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    path: &Path,
) {
    schematic_overlay_close(state);
    // `checkTags(s)`: every new label joins the global tag list.
    let mut added = false;
    if let Some(schematic) = schematic_read_file(path) {
        let mut st = state.borrow_mut();
        for label in &schematic.labels {
            if !st.tags.iter().any(|tag| tag == label) {
                st.tags.push(label.clone());
                added = true;
            }
        }
    }
    if added {
        schematics_save_tags(state);
    }
    schematics_rebuild_tags(dialog, state);
    schematics_rebuild_list(dialog, state);
    let saved = kit::t("@schematic.saved").to_string();
    let _ = ui_call("show_info_fade", &[gs(saved.as_str()).to_variant()]);
    let index = schematic_index_by_file(dialog, path);
    if index >= 0 {
        schematics_open_info(index);
    }
}

/// `showExport` clipboard half: `ui.showInfoFade("@copied")` +
/// `Core.app.setClipboardText(schematics.writeBase64(s))`.
fn schematics_export_clipboard(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
) {
    let facade = campaign_schematic_mutation(dialog, "export_schematic", &[index.to_variant()])
        .filter(|value| value.get_type() != VariantType::NIL)
        .map(|value| to_text_godot(&value))
        .unwrap_or_default();
    let text = if !facade.is_empty() {
        facade
    } else {
        let Some(path) = schematic_file_of(dialog, index) else {
            let none = kit::t("@none").to_string();
            dialog.show_toast(&none);
            return;
        };
        let Some(text) = schematic_base64_file(&path) else {
            let none = kit::t("@none").to_string();
            dialog.show_toast(&none);
            return;
        };
        text
    };
    DisplayServer::singleton().clipboard_set(&gs(text.as_str()));
    let copied = kit::t("@copied").to_string();
    let _ = ui_call("show_info_fade", &[gs(copied.as_str()).to_variant()]);
    schematic_overlay_close(state);
}

/// `showExport` file half: `FileChooser.export(s.name(), schematicExtension,
/// file -> Schematics.write(s, file))`.
fn schematics_export_file(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
    path: &Path,
) {
    let Some(source) = schematic_file_of(dialog, index) else {
        return;
    };
    let written = match NativeFs.read(&source) {
        Ok(bytes) => NativeFs.write(path, &bytes).is_ok(),
        Err(_) => false,
    };
    if !written {
        let text = kit::t("@error.title").to_string();
        dialog.show_toast(&text);
        return;
    }
    schematic_overlay_close(state);
}

/// `FileChooser.open(schematicExtension).submitMulti(...)`.
fn schematics_pick_import_files(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let target = dialog.to_gd();
    let state_for = state.clone();
    schematic_file_dialog(
        dialog,
        state,
        "@import.file",
        FileMode::OPEN_FILES,
        "",
        move |files| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_import_files(&mut bound, &state_for, &files);
        },
    );
}

/// `FileChooser.export(s.name(), schematicExtension, ...)`.
fn schematics_pick_export_file(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
    name: &str,
) {
    let target = dialog.to_gd();
    let state_for = state.clone();
    let default_name = format!(
        "{}.{}",
        mind_core::util::strings::sanitize_filename(name),
        mind_core::game::rules::SCHEMATIC_EXTENSION
    );
    schematic_file_dialog(
        dialog,
        state,
        "@export.file",
        FileMode::SAVE_FILE,
        &default_name,
        move |files| {
            let Some(path) = files.first().cloned() else {
                return;
            };
            let mut path = PathBuf::from(path);
            if path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| !extension.eq_ignore_ascii_case("msch"))
                .unwrap_or(true)
            {
                path.set_extension(mind_core::game::rules::SCHEMATIC_EXTENSION);
            }
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_export_file(&mut bound, &state_for, index, &path);
        },
    );
}

/// The native `.msch` file dialog (`FileChooser`; a runtime `FileDialog`).
fn schematic_file_dialog(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    title_key: &str,
    mode: FileMode,
    default_name: &str,
    on_files: impl Fn(Vec<String>) + 'static,
) {
    schematic_file_chooser_close(state);
    // code-instantiated: `FileChooser` is a runtime native dialog.
    let mut chooser = FileDialog::new_alloc();
    chooser.set_title(&kit::t(title_key));
    chooser.set_file_mode(mode);
    chooser.set_access(Access::FILESYSTEM);
    chooser.add_filter(&gs(format!(
        "*.{}",
        mind_core::game::rules::SCHEMATIC_EXTENSION
    )));
    if !default_name.is_empty() {
        chooser.set_current_file(&gs(default_name));
    }
    let handler = Rc::new(on_files);
    let state_for_selected = state.clone();
    let single_chooser = chooser.clone();
    let single = handler.clone();
    let callable = Callable::from_fn("schematic_file_selected", move |args: &[&Variant]| {
        state_for_selected.borrow_mut().file_chooser = None;
        let mut chooser = single_chooser.clone();
        chooser.queue_free();
        let path = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        if !path.is_empty() {
            single(vec![path]);
        }
    });
    chooser.connect("file_selected", &callable);
    let state_for_selected_multi = state.clone();
    let state_for_cancel = state.clone();
    let multi_chooser = chooser.clone();
    let multi = handler.clone();
    let callable = Callable::from_fn("schematic_files_selected", move |args: &[&Variant]| {
        state_for_selected_multi.borrow_mut().file_chooser = None;
        let mut chooser = multi_chooser.clone();
        chooser.queue_free();
        let paths = args
            .first()
            .and_then(|value| (*value).try_to::<PackedStringArray>().ok())
            .unwrap_or_default();
        multi(
            paths
                .as_slice()
                .iter()
                .map(|path| path.to_string())
                .collect(),
        );
    });
    chooser.connect("files_selected", &callable);
    let cancel_chooser = chooser.clone();
    let callable = Callable::from_fn("schematic_file_canceled", move |_args: &[&Variant]| {
        state_for_cancel.borrow_mut().file_chooser = None;
        let mut chooser = cancel_chooser.clone();
        chooser.queue_free();
    });
    chooser.connect("canceled", &callable);
    state.borrow_mut().file_chooser = Some(chooser.clone());
    let mut node: Gd<Node> = dialog.to_gd().upcast();
    node.add_child(&chooser);
    chooser.popup_centered_ratio_ex().ratio(0.6).done();
}
