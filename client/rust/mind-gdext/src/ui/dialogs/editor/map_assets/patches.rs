// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The patches view of the map assets dialog.
//! Upstream: editor/data/MapAssetsDialog.java (`MapPatchesView`)

use super::*;
use crate::gdutil::variant::{dict_i64_or, dict_text_or};

pub(super) fn map_assets_build_patches(
    dialog: &Gd<MindDialog>,
    list: &mut Gd<VBoxContainer>,
    state: &Rc<RefCell<MapAssetsState>>,
) {
    let rows = map_assets_rows_of(state, "patch");
    let mut shown_count = 0;
    for row in &rows {
        if !map_assets_matches(state, row, "name") {
            continue;
        }
        shown_count += 1;
        let widget = map_assets_patch_row(dialog, state, row);
        list.add_child(&widget);
    }
    if shown_count == 0 {
        let label = factory::label(&markup("@patch.none"));
        list.add_child(&label);
    }
}

pub(super) fn map_assets_patch_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
) -> Gd<HBoxContainer> {
    let index = dict_i64_or(row, "index", 0);
    let patch_name = dict_text_or(row, "name");
    let json = dict_text_or(row, "data");
    let label = if patch_name.is_empty() {
        String::from("<unnamed>")
    } else {
        patch_name
    };
    let fields = map_assets_count_fields(&map_assets_parse_json(&json));
    // code-instantiated: one patch entry row (show/copy/import/delete).
    let mut box_ = factory::hbox(4);
    box_.set_alignment(AlignmentMode::CENTER);
    box_.add_child(&map_assets_empty_cell());
    let width = ASSET_ROW_WIDTH;
    let title_for_show = label.clone();
    let json_for_show = json.clone();
    let show = Callable::from_fn("patch_show", move |_args: &[&Variant]| {
        let title = kit::tm_format("@patch.set", &[&title_for_show]).to_string();
        map_assets_show_text(&title, &json_for_show);
    });
    let subtitle = kit::tm_format("@patch.fields", &[&fields.to_string()]).to_string();
    box_.add_child(&map_assets_entry_button(
        row, &label, &subtitle, width, show,
    ));
    let json_for_copy = json.clone();
    let copy = Callable::from_fn("patch_copy", move |_args: &[&Variant]| {
        DisplayServer::singleton().clipboard_set(&GString::from(json_for_copy.as_str()));
        show_toast(&kit::t("@copied").to_string());
    });
    box_.add_child(&square_cell(
        "copy",
        &kit::t("@copy.clipboard").to_string(),
        copy,
    ));
    let state_for_import = state.clone();
    let dialog_for_import = dialog.clone();
    let replace = Callable::from_fn("patch_replace", move |_args: &[&Variant]| {
        let state_for = state_for_import.clone();
        let dialog_for = dialog_for_import.clone();
        map_assets_import_choices(
            &dialog_for.clone(),
            map_assets_text_filters(),
            Rc::new(move |text: String| {
                let result = editor_call(
                    "data_patch_replace",
                    &[
                        index.to_variant(),
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
        replace,
    ));
    let state_for_delete = state.clone();
    let dialog_for_delete = dialog.clone();
    let delete = Callable::from_fn("patch_delete", move |_args: &[&Variant]| {
        let state_for = state_for_delete.clone();
        let message = kit::t("@patch.delete.confirm").to_string();
        map_assets_confirm(
            &dialog_for_delete,
            &state_for,
            &message,
            PendingConfirm::PatchDelete(index),
        );
    });
    box_.add_child(&square_cell(
        "trash",
        &kit::t("@patch.delete.confirm").to_string(),
        delete,
    ));
    box_
}

/// `_add_patch`: `MapPatchesView.addPatch` through the clipboard/file import.
pub(super) fn map_assets_add_patch(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    map_assets_import_choices(
        dialog,
        map_assets_text_filters(),
        Rc::new(move |text: String| {
            let result = editor_call(
                "data_patch_add",
                &[GString::from(text.as_str()).to_variant()],
            );
            map_assets_apply(&dialog_for, &state_for, result);
        }),
    );
}

/// `_text_filters()`: the patch JSON extensions.
pub(super) fn map_assets_text_filters() -> Vec<String> {
    vec![
        String::from("*.json"),
        String::from("*.hjson"),
        String::from("*.json5"),
    ]
}

/// `MapPatchesView.countFields`: leaf count of the patch JSON, at least 1.
pub(super) fn map_assets_count_fields(value: &Variant) -> i64 {
    if let Ok(dict) = value.try_to::<VarDictionary>() {
        let sum: i64 = dict
            .values_array()
            .iter_shared()
            .map(|child| map_assets_count_fields(&child))
            .sum();
        return sum.max(1);
    }
    if let Ok(array) = value.try_to::<VarArray>() {
        let sum: i64 = array
            .iter_shared()
            .map(|child| map_assets_count_fields(&child))
            .sum();
        return sum.max(1);
    }
    1
}

/// `_parse_json(text)` (`{}` when unparseable).
pub(super) fn map_assets_parse_json(text: &str) -> Variant {
    if text.is_empty() {
        return VarDictionary::new().to_variant();
    }
    let parsed = Json::parse_string(text);
    if parsed.is_nil() {
        return VarDictionary::new().to_variant();
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dialogs::editor::util::tests::engine_available;

    /// PAR-0586: `map_assets_parse_json`/`map_assets_count_fields` are Godot
    /// builtin calls (`Json.parse_string`, `VarDictionary.values_array`), so
    /// the leaf-count rule is exercised only in-engine.
    #[test]
    fn count_fields_counts_leaves() {
        if !engine_available() {
            return;
        }
        let json = r#"{"a": 1, "b": {"c": 2, "d": 3}}"#;
        let parsed = map_assets_parse_json(json);
        assert_eq!(map_assets_count_fields(&parsed), 3);
        assert_eq!(map_assets_count_fields(&VarArray::new().to_variant()), 1);
    }
}
