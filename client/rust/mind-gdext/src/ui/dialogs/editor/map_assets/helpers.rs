// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The helpers view of the map assets dialog.
//! Upstream: editor/data/MapAssetsDialog.java (shared helpers of the data-asset views)

use super::*;

/// `types.image(Tex.whiteui, Pal.accent).size(4f, 50f)`.
pub(super) fn map_assets_divider() -> Gd<ColorRect> {
    // code-instantiated: the 4x50 accent rule between type-strip groups.
    let mut line = ColorRect::new_alloc();
    line.set_color(styles::ACCENT);
    line.set_custom_minimum_size(Vector2::new(4.0, 50.0));
    line
}

/// Rows of one `DataAssetType` (`_rows_of`).
pub(super) fn map_assets_rows_of(
    state: &Rc<RefCell<MapAssetsState>>,
    kind: &str,
) -> Vec<VarDictionary> {
    state
        .borrow()
        .rows
        .iter()
        .filter(|row| dict_string(row, "kind") == kind)
        .cloned()
        .collect()
}

/// `_matches`: the lowercased search over one row key.
pub(super) fn map_assets_matches(
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
    key: &str,
) -> bool {
    let search = state.borrow().search.clone();
    if search.is_empty() {
        return true;
    }
    dict_string(row, key).to_lowercase().contains(&search)
}

/// The viewport width from a live dialog handle.
pub(super) fn map_assets_viewport_width(dialog: &Gd<MindDialog>) -> f32 {
    dialog.bind().base().get_viewport_rect().size.x
}

/// `_row_width`: `Vars.mobile ? 390f : 450f`.
pub(super) fn map_assets_row_width(dialog: &MindDialog) -> f32 {
    if is_portrait(dialog) {
        ASSET_ROW_WIDTH_MOBILE
    } else {
        ASSET_ROW_WIDTH
    }
}

/// Empty cell for Java's `list.add().size(h)` warning placeholder.
pub(super) fn map_assets_empty_cell() -> Gd<Control> {
    factory::space(ASSET_ROW_HEIGHT, ASSET_ROW_HEIGHT)
}

/// The two-line entry button (`[accent]name\n[lightgray]sub`).
pub(super) fn map_assets_entry_button(
    row: &VarDictionary,
    title: &str,
    subtitle: &str,
    width: f32,
    handler: Callable,
) -> Gd<Button> {
    // code-instantiated: one two-line data-asset entry row.
    let mut button = factory::button("");
    button.set_theme_type_variation("grayt");
    button.set_custom_minimum_size(Vector2::new(width, ASSET_ROW_HEIGHT));
    button.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut raw = format!("[accent]{title}");
    if !subtitle.is_empty() {
        raw.push_str(&format!("\n[lightgray]{subtitle}"));
    }
    let mut label = factory::label(&markup(&raw));
    label.set_horizontal_alignment(HorizontalAlignment::LEFT);
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    label.set_offset(Side::LEFT, 10.0);
    label.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&label);
    button.set_tooltip_text(&dict_string(row, "path"));
    button.connect("pressed", &handler);
    button
}

/// `_error(message)`: the shared error prompt (`@error.title`).
pub(super) fn map_assets_error(message: &str) {
    let title = kit::t("@error.title").to_string();
    if ui_call(
        "show_text",
        &[
            GString::from(title.as_str()).to_variant(),
            GString::from(message).to_variant(),
        ],
    )
    .is_some()
    {
        return;
    }
    log::warn!("[ui] map_assets: {message}");
}

/// `ui.showConfirm` + one pending handler (`confirm_result`).
pub(super) fn map_assets_confirm(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    message: &str,
    pending: PendingConfirm,
) {
    if ui_call("show_confirm", &[GString::from(message).to_variant()]).is_some() {
        state.borrow_mut().pending_confirm = Some(pending);
        return;
    }
    map_assets_run_confirm(dialog, state, pending);
}

pub(super) fn map_assets_on_confirm(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    confirmed: bool,
) {
    let pending = state.borrow_mut().pending_confirm.take();
    if !confirmed {
        return;
    }
    if let Some(pending) = pending {
        map_assets_run_confirm(dialog, state, pending);
    }
}

pub(super) fn map_assets_run_confirm(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    pending: PendingConfirm,
) {
    match pending {
        PendingConfirm::ClearAll => {
            let result = editor_call("data_clear", &[]);
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::PatchDelete(index) => {
            let result = editor_call("data_patch_remove", &[index.to_variant()]);
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::ContentDelete(name) => {
            let result = editor_call(
                "data_asset_delete",
                &[
                    GString::from("content").to_variant(),
                    GString::from(name.as_str()).to_variant(),
                ],
            );
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::BundleDelete(path) => {
            let result = editor_call(
                "data_asset_delete",
                &[
                    GString::from("bundle").to_variant(),
                    GString::from(path.as_str()).to_variant(),
                ],
            );
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::ImageDelete(name) => {
            let result = editor_call(
                "data_asset_delete",
                &[
                    GString::from("image").to_variant(),
                    GString::from(name.as_str()).to_variant(),
                ],
            );
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::AudioDelete(kind, name) => {
            let result = editor_call(
                "data_asset_delete",
                &[
                    GString::from(kind.as_str()).to_variant(),
                    GString::from(name.as_str()).to_variant(),
                ],
            );
            map_assets_apply(dialog, state, result);
        }
    }
}

/// Owner error codes -> bundle keys (`DataManager` dialog messages).
pub(super) fn map_assets_error_key(code: &str) -> Option<&'static str> {
    match code {
        "exists" => Some("@asset.content.exists"),
        "badname" => Some("@asset.content.badname"),
        "badtype" => Some("@asset.content.type"),
        "invalid JSON" => Some("@patch.importerror"),
        _ => None,
    }
}

/// Runs an owner action; on failure shows its message, on success refetches.
pub(super) fn map_assets_apply(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    result: Option<Variant>,
) {
    let value = result
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default();
    if !dict_bool(&value, "ok", false) {
        let mut message = dict_string(&value, "error");
        if let Some(key) = map_assets_error_key(&message) {
            message = kit::t(key).to_string();
        }
        map_assets_error(&message);
        return;
    }
    map_assets_refetch(state);
    map_assets_rebuild(dialog, state);
}

/// `_show_text(title, text_body)`.
pub(super) fn map_assets_show_text(title: &str, body: &str) {
    show_text(title, body);
}

/// `_open_file(label, filters, multiple, handler)`: the runtime `FileDialog`.
pub(super) fn map_assets_open_file(
    dialog: &Gd<MindDialog>,
    label: &str,
    filters: Vec<String>,
    multiple: bool,
    handler: AssetPathsHandler,
) {
    // code-instantiated: `FileChooser.open` is a runtime native dialog.
    let mut file_dialog = FileDialog::new_alloc();
    file_dialog.set_title(&GString::from(label));
    file_dialog.set_file_mode(if multiple {
        FileMode::OPEN_FILES
    } else {
        FileMode::OPEN_FILE
    });
    file_dialog.set_access(Access::FILESYSTEM);
    for filter in &filters {
        file_dialog.add_filter(&GString::from(filter.as_str()));
    }
    let selected_target = file_dialog.clone();
    let selected_handler = handler.clone();
    let callable = Callable::from_fn("asset_file_selected", move |args: &[&Variant]| {
        let path = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut target = selected_target.clone();
        target.queue_free();
        selected_handler(vec![path]);
    });
    file_dialog.connect("file_selected", &callable);
    let files_target = file_dialog.clone();
    let files_handler = handler.clone();
    let callable = Callable::from_fn("asset_files_selected", move |args: &[&Variant]| {
        let paths = args
            .first()
            .and_then(|value| (*value).try_to::<PackedStringArray>().ok())
            .unwrap_or_default();
        let mut target = files_target.clone();
        target.queue_free();
        files_handler(
            paths
                .as_slice()
                .iter()
                .map(|path| path.to_string())
                .collect(),
        );
    });
    file_dialog.connect("files_selected", &callable);
    let canceled_target = file_dialog.clone();
    let callable = Callable::from_fn("asset_file_canceled", move |_args: &[&Variant]| {
        let mut target = canceled_target.clone();
        target.queue_free();
    });
    file_dialog.connect("canceled", &callable);
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&file_dialog);
    file_dialog.popup_centered_ratio_ex().ratio(0.6).done();
}

/// `_save_file(label, file_name, filter, handler)`: the runtime save dialog.
pub(super) fn map_assets_save_file(
    dialog: &Gd<MindDialog>,
    label: &str,
    file_name: &str,
    filter: &str,
    handler: AssetTextHandler,
) {
    // code-instantiated: `FileChooser.export` is a runtime native dialog.
    let mut file_dialog = FileDialog::new_alloc();
    file_dialog.set_title(&GString::from(label));
    file_dialog.set_file_mode(FileMode::SAVE_FILE);
    file_dialog.set_access(Access::FILESYSTEM);
    file_dialog.set_current_file(&GString::from(file_name));
    file_dialog.add_filter(&GString::from(filter));
    let selected_target = file_dialog.clone();
    let callable = Callable::from_fn("asset_save_selected", move |args: &[&Variant]| {
        let path = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut target = selected_target.clone();
        target.queue_free();
        handler(path);
    });
    file_dialog.connect("file_selected", &callable);
    let canceled_target = file_dialog.clone();
    let callable = Callable::from_fn("asset_save_canceled", move |_args: &[&Variant]| {
        let mut target = canceled_target.clone();
        target.queue_free();
    });
    file_dialog.connect("canceled", &callable);
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&file_dialog);
    file_dialog.popup_centered_ratio_ex().ratio(0.6).done();
}

/// `_read_text(path)`: `FileAccess.get_file_as_string`.
pub(super) fn map_assets_read_text(path: &str) -> String {
    FileAccess::get_file_as_string(path).to_string()
}

/// `_to_array(paths)`.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub(super) fn map_assets_to_array(values: &PackedStringArray) -> Vec<String> {
    values
        .as_slice()
        .iter()
        .map(|value| value.to_string())
        .collect()
}

/// `_write_text(path, text)` (`FileAccess.open(path, WRITE).store_string`).
pub(super) fn map_assets_write_text(path: &str, text: &str) {
    let Some(mut file) = FileAccess::open(path, godot::classes::file_access::ModeFlags::WRITE)
    else {
        return;
    };
    let _ = file.store_string(text);
}

/// `showImport(handler)`: clipboard + file choices (`MapPatchesView`).
pub(super) fn map_assets_import_choices(
    dialog: &Gd<MindDialog>,
    filters: Vec<String>,
    handler: AssetImportHandler,
) {
    // code-instantiated: Java's `BaseDialog("@editor.import")` choice menu.
    let mut accept = AcceptDialog::new_alloc();
    accept.set_title(&kit::t("@editor.import"));
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 6);
    box_.set_custom_minimum_size(Vector2::new(300.0, 120.0));
    let clipboard_text = DisplayServer::singleton().clipboard_get();
    let clipboard_label = kit::t("@load.clipboard").to_string();
    let mut clipboard = factory::icon_button(
        "copy",
        &clipboard_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    clipboard.set_disabled(clipboard_text.is_empty());
    let clip_target = accept.clone();
    let clip_handler = handler.clone();
    let callable = Callable::from_fn("asset_import_clipboard", move |_args: &[&Variant]| {
        let text = DisplayServer::singleton().clipboard_get().to_string();
        let mut target = clip_target.clone();
        target.queue_free();
        if !text.is_empty() {
            clip_handler(text);
        }
    });
    clipboard.connect("pressed", &callable);
    box_.add_child(&clipboard);
    let file_label = kit::t("@import.file").to_string();
    let mut file = factory::icon_button(
        "download",
        &file_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let file_target = accept.clone();
    let dialog_for = dialog.clone();
    let file_handler = handler.clone();
    let file_filters = filters.clone();
    let callable = Callable::from_fn("asset_import_file", move |_args: &[&Variant]| {
        let mut target = file_target.clone();
        target.queue_free();
        let handler_for = file_handler.clone();
        let label = kit::t("@import.file").to_string();
        map_assets_open_file(
            &dialog_for,
            &label,
            file_filters.clone(),
            false,
            Rc::new(move |paths: Vec<String>| {
                let path = paths.first().cloned().unwrap_or_default();
                let text = map_assets_read_text(&path);
                handler_for(text);
            }),
        );
    });
    file.connect("pressed", &callable);
    box_.add_child(&file);
    accept.add_child(&box_);
    if let Some(mut ok) = accept.get_ok_button() {
        ok.set_visible(false);
    }
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&accept);
    accept
        .popup_centered_ex()
        .minsize(Vector2i::new(340, 180))
        .done();
}

/// `MapAssetsDialog.showEditMenu` (`@asset.importzip`/`exportzip`/`clearall`).
pub(super) fn map_assets_show_edit_menu(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
) {
    // code-instantiated: Java builds this `BaseDialog("@edit.menu")` per open.
    let mut accept = AcceptDialog::new_alloc();
    accept.set_title(&kit::t("@edit.menu"));
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 6);
    box_.set_custom_minimum_size(Vector2::new(300.0, 170.0));

    let import_label = kit::t("@asset.importzip").to_string();
    let mut import_zip = factory::icon_button(
        "download",
        &import_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let import_target = accept.clone();
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("asset_import_zip", move |_args: &[&Variant]| {
        let mut target = import_target.clone();
        target.queue_free();
        let label = kit::t("@asset.importzip").to_string();
        let dialog_for_zip = dialog_for.clone();
        let state_for_zip = state_for.clone();
        map_assets_open_file(
            &dialog_for,
            &label,
            vec![String::from("*.zip")],
            false,
            Rc::new(move |paths: Vec<String>| {
                let path = paths.first().cloned().unwrap_or_default();
                let result = editor_call(
                    "data_import_zip",
                    &[GString::from(path.as_str()).to_variant()],
                );
                map_assets_apply(&dialog_for_zip, &state_for_zip, result);
            }),
        );
    });
    import_zip.connect("pressed", &callable);
    box_.add_child(&import_zip);

    let export_label = kit::t("@asset.exportzip").to_string();
    let mut export_zip = factory::icon_button(
        "upload",
        &export_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let export_target = accept.clone();
    let dialog_for = dialog.clone();
    let callable = Callable::from_fn("asset_export_zip", move |_args: &[&Variant]| {
        let mut target = export_target.clone();
        target.queue_free();
        let label = kit::t("@asset.exportzip").to_string();
        map_assets_save_file(
            &dialog_for,
            &label,
            "assets.zip",
            "*.zip",
            Rc::new(move |path: String| {
                let exported = editor_call(
                    "data_export_zip",
                    &[GString::from(path.as_str()).to_variant()],
                )
                .map(|value| variant_bool(&value))
                .unwrap_or(false);
                if !exported {
                    map_assets_error(&kit::t("@asset.exportzip").to_string());
                }
            }),
        );
    });
    export_zip.connect("pressed", &callable);
    box_.add_child(&export_zip);

    let clear_label = kit::t("@asset.clearall").to_string();
    let mut clear_all = factory::icon_button(
        "trash",
        &clear_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let clear_target = accept.clone();
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("asset_clear_all", move |_args: &[&Variant]| {
        let mut target = clear_target.clone();
        target.queue_free();
        let message = kit::t("@asset.clearall.confirm").to_string();
        map_assets_confirm(&dialog_for, &state_for, &message, PendingConfirm::ClearAll);
    });
    clear_all.connect("pressed", &callable);
    box_.add_child(&clear_all);
    accept.add_child(&box_);
    if let Some(mut ok) = accept.get_ok_button() {
        ok.set_visible(false);
    }
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&accept);
    accept
        .popup_centered_ex()
        .minsize(Vector2i::new(340, 230))
        .done();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dialogs::schematics::schematic_info::auto_fixed;

    #[test]
    fn error_codes_map_to_bundle_keys() {
        assert_eq!(
            map_assets_error_key("exists"),
            Some("@asset.content.exists")
        );
        assert_eq!(
            map_assets_error_key("badname"),
            Some("@asset.content.badname")
        );
        assert_eq!(map_assets_error_key("badtype"), Some("@asset.content.type"));
        assert_eq!(
            map_assets_error_key("invalid JSON"),
            Some("@patch.importerror")
        );
        assert_eq!(map_assets_error_key("other"), None);
    }
    #[test]
    fn auto_fixed_drops_trailing_zeros() {
        assert_eq!(auto_fixed(120.0, 2), "120");
        assert_eq!(auto_fixed(1.5, 2), "1.5");
        assert_eq!(auto_fixed(0.0, 2), "0");
    }
}
