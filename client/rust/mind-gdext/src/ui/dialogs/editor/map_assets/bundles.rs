// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The bundles view of the map assets dialog.
//! Upstream: editor/data/MapAssetsDialog.java (`MapBundlesView`)

use super::*;
use crate::gdutil::variant::{dict_bool_or, dict_i64_or, dict_text_or};

pub(super) fn map_assets_build_bundles(
    dialog: &Gd<MindDialog>,
    list: &mut Gd<VBoxContainer>,
    state: &Rc<RefCell<MapAssetsState>>,
) {
    let rows = map_assets_rows_of(state, "bundle");
    let mut shown_count = 0;
    for row in &rows {
        if !map_assets_matches(state, row, "name") {
            continue;
        }
        shown_count += 1;
        let widget = map_assets_bundle_row(dialog, state, row);
        list.add_child(&widget);
    }
    if shown_count == 0 {
        let label = factory::label(&markup("@patch.none"));
        list.add_child(&label);
    }
}

pub(super) fn map_assets_bundle_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
) -> Gd<HBoxContainer> {
    let bundle_name = dict_text_or(row, "name");
    let path = dict_text_or(row, "path");
    let text = dict_text_or(row, "data");
    let lines = dict_i64_or(row, "size", 0);
    // code-instantiated: one bundle entry row (show/import/export/delete).
    let mut box_ = factory::hbox(4);
    box_.set_alignment(AlignmentMode::CENTER);
    box_.add_child(&map_assets_empty_cell());
    let name_for_show = bundle_name.clone();
    let text_for_show = text.clone();
    let show = Callable::from_fn("bundle_show", move |_args: &[&Variant]| {
        map_assets_show_text(&name_for_show, &text_for_show);
    });
    let subtitle = kit::tm_format("@bundle.lines", &[&lines.to_string()]).to_string();
    let width = map_assets_row_width(&dialog.bind());
    box_.add_child(&map_assets_entry_button(
        row,
        &bundle_name,
        &subtitle,
        width,
        show,
    ));

    let state_for_import = state.clone();
    let dialog_for_import = dialog.clone();
    let path_for_import = path.clone();
    let refresh = Callable::from_fn("bundle_import", move |_args: &[&Variant]| {
        let state_for = state_for_import.clone();
        let dialog_for = dialog_for_import.clone();
        let path_for = path_for_import.clone();
        map_assets_import_choices(
            &dialog_for.clone(),
            vec![String::from("*.properties"), String::from("*.txt")],
            Rc::new(move |text: String| {
                let result = editor_call(
                    "data_bundle_set",
                    &[
                        GString::from(path_for.as_str()).to_variant(),
                        GString::from(text.as_str()).to_variant(),
                    ],
                );
                map_assets_apply(&dialog_for, &state_for, result);
            }),
        );
    });
    box_.add_child(&square_cell(
        "refresh",
        &kit::t("@editor.import").to_string(),
        refresh,
    ));

    let dialog_for_export = dialog.clone();
    let bundle_for_export = bundle_name.clone();
    let text_for_export = text.clone();
    let export = Callable::from_fn("bundle_export", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_export.clone();
        let bundle_for = bundle_for_export.clone();
        let text_for = text_for_export.clone();
        let label = kit::t("@editor.export").to_string();
        map_assets_save_file(
            &dialog_for,
            &label,
            &bundle_for,
            "*.properties",
            Rc::new(move |target: String| {
                map_assets_write_text(&target, &text_for);
            }),
        );
    });
    box_.add_child(&square_cell(
        "export",
        &kit::t("@export.file").to_string(),
        export,
    ));

    let state_for_delete = state.clone();
    let dialog_for_delete = dialog.clone();
    let path_for_delete = path.clone();
    let delete = Callable::from_fn("bundle_delete", move |_args: &[&Variant]| {
        let state_for = state_for_delete.clone();
        let message = kit::t("@asset.delete.confirm").to_string();
        map_assets_confirm(
            &dialog_for_delete,
            &state_for,
            &message,
            PendingConfirm::BundleDelete(path_for_delete.clone()),
        );
    });
    box_.add_child(&square_cell(
        "trash",
        &kit::t("@asset.delete.confirm").to_string(),
        delete,
    ));
    box_
}

pub(super) fn map_assets_add_bundle(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let label = kit::t("@add").to_string();
    map_assets_open_file(
        dialog,
        &label,
        vec![String::from("*.properties"), String::from("*.txt")],
        true,
        Rc::new(move |paths: Vec<String>| {
            for path in paths {
                let result = editor_call(
                    "data_file_add",
                    &[
                        GString::from("bundle").to_variant(),
                        GString::from(path.as_str()).to_variant(),
                    ],
                )
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .unwrap_or_default();
                if !dict_bool_or(&result, "ok", false) {
                    map_assets_error(&dict_text_or(&result, "error"));
                }
            }
            map_assets_refetch(&state_for);
            map_assets_rebuild(&dialog_for, &state_for);
        }),
    );
}
