// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The content view of the map assets dialog.
//! Upstream: editor/data/MapAssetsDialog.java (`MapContentView`)

use super::*;
use crate::gdutil::variant::{dict_bool_or, dict_text_or};

pub(super) fn map_assets_build_content(
    dialog: &Gd<MindDialog>,
    list: &mut Gd<VBoxContainer>,
    state: &Rc<RefCell<MapAssetsState>>,
) {
    let rows = map_assets_rows_of(state, "content");
    let mut shown_count = 0;
    let mut last_type = String::new();
    for row in &rows {
        if !map_assets_matches(state, row, "name") {
            continue;
        }
        shown_count += 1;
        let content_type = dict_text_or(row, "contentType");
        if content_type != last_type {
            last_type = content_type.clone();
            let header = map_assets_content_header(&content_type);
            list.add_child(&header);
        }
        let widget = map_assets_content_row(dialog, state, row, &content_type);
        list.add_child(&widget);
    }
    if shown_count == 0 {
        let label = factory::label(&markup("@none.found"));
        list.add_child(&label);
    }
}

pub(super) fn map_assets_content_header(content_type: &str) -> Gd<HBoxContainer> {
    // `list.table(t -> icon + "@content.<type>" + whiteui rule).colspan(6)`.
    // code-instantiated: one content-type header row.
    let mut box_ = factory::hbox(8);
    box_.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let glyph = factory::glyph_label(asset_content_icon(content_type), 22, LIGHT_GRAY);
    box_.add_child(&glyph);
    let mut label = factory::label(&kit::t(&format!("@content.{content_type}")).to_string());
    label.add_theme_color_override("font_color", LIGHT_GRAY);
    box_.add_child(&label);
    // code-instantiated: the rule under the type header (Java `image().growX()`).
    let mut rule = ColorRect::new_alloc();
    rule.set_color(LIGHT_GRAY);
    rule.set_custom_minimum_size(Vector2::new(0.0, 4.0));
    rule.set_h_size_flags(SizeFlags::EXPAND_FILL);
    box_.add_child(&rule);
    box_
}

pub(super) fn map_assets_content_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
    content_type: &str,
) -> Gd<HBoxContainer> {
    let content_name = dict_text_or(row, "name");
    let json = dict_text_or(row, "data");
    // code-instantiated: one content record row.
    let mut box_ = factory::hbox(4);
    box_.set_alignment(AlignmentMode::CENTER);
    box_.add_child(&map_assets_empty_cell());

    // `content.content instanceof UnlockableContent ? uiIcon : contentIcons.get(type)`.
    let mut icon = factory::icon_button(
        asset_content_icon(content_type),
        "",
        "graySquarei",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    icon.set_custom_minimum_size(Vector2::new(ASSET_ROW_HEIGHT, ASSET_ROW_HEIGHT));
    icon.set_disabled(true);
    box_.add_child(&icon);

    let name_for_show = content_name.clone();
    let json_for_show = json.clone();
    let show = Callable::from_fn("content_show", move |_args: &[&Variant]| {
        map_assets_show_text(&name_for_show, &json_for_show);
    });
    let subtitle = kit::t(&format!("@content.{content_type}")).to_string();
    let width = ASSET_ROW_WIDTH;
    box_.add_child(&map_assets_entry_button(
        row,
        &content_name,
        &subtitle,
        width,
        show,
    ));

    let state_for_import = state.clone();
    let dialog_for_import = dialog.clone();
    let content_for_import = content_name.clone();
    let type_for_import = String::from(content_type);
    let replace = Callable::from_fn("content_replace", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_import.clone();
        let state_for = state_for_import.clone();
        let content_for = content_for_import.clone();
        let type_for = type_for_import.clone();
        map_assets_import_choices(
            &dialog_for.clone(),
            map_assets_text_filters(),
            Rc::new(move |text: String| {
                // Java overwrites `content.data`; replace then re-add.
                let removed = editor_call(
                    "data_asset_delete",
                    &[
                        GString::from("content").to_variant(),
                        GString::from(content_for.as_str()).to_variant(),
                    ],
                )
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .unwrap_or_default();
                if !dict_bool_or(&removed, "ok", false) {
                    map_assets_error(&dict_text_or(&removed, "error"));
                    return;
                }
                let result = editor_call(
                    "data_content_add",
                    &[
                        GString::from(content_for.as_str()).to_variant(),
                        GString::from(type_for.as_str()).to_variant(),
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

    let dialog_for_export = dialog.clone();
    let content_for_export = content_name.clone();
    let json_for_export = json.clone();
    let export = Callable::from_fn("content_export", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_export.clone();
        let content_for = content_for_export.clone();
        let json_for = json_for_export.clone();
        let label = kit::t("@editor.export").to_string();
        map_assets_save_file(
            &dialog_for,
            &label,
            &format!("{content_for}.json"),
            "*.json",
            Rc::new(move |path: String| {
                map_assets_write_text(&path, &json_for);
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
    let content_for_delete = content_name.clone();
    let delete = Callable::from_fn("content_delete", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_delete.clone();
        let state_for = state_for_delete.clone();
        let message = kit::t("@asset.delete.confirm").to_string();
        map_assets_confirm(
            &dialog_for,
            &state_for,
            &message,
            PendingConfirm::ContentDelete(content_for_delete.clone()),
        );
    });
    box_.add_child(&square_cell(
        "trash",
        &kit::t("@asset.delete.confirm").to_string(),
        delete,
    ));
    box_
}

/// `MapContentView.showImport`: name + type row, then clipboard/file.
pub(super) fn map_assets_add_content(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    // code-instantiated: Java's `BaseDialog("@asset.content.add")` name/type form.
    let mut accept = AcceptDialog::new_alloc();
    accept.set_title(&kit::t("@asset.content.add"));
    let mut box_ = factory::vbox(6);
    box_.set_custom_minimum_size(Vector2::new(420.0, 190.0));
    let mut name_row = HBoxContainer::new_alloc();
    let name_label = factory::label(&kit::t("@name").to_string());
    name_row.add_child(&name_label);
    let mut name_field = factory::field("");
    name_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    name_field.set_max_length(64);
    name_row.add_child(&name_field);
    box_.add_child(&name_row);
    let mut type_row = HBoxContainer::new_alloc();
    let type_label = factory::label(&kit::t("@asset.content.type").to_string());
    type_row.add_child(&type_label);
    let selection: Rc<RefCell<String>> = Rc::new(RefCell::new(String::from("item")));
    let type_buttons: Rc<RefCell<Vec<Gd<Button>>>> = Rc::new(RefCell::new(Vec::new()));
    for content_type in ASSET_CONTENT_TYPES {
        let content_type = String::from(content_type);
        let mut button = factory::glyph_button(
            asset_content_icon(&content_type),
            &kit::t(&format!("@content.{content_type}")).to_string(),
            "squareTogglei",
        );
        button.set_toggle_mode(true);
        button.set_pressed(*selection.borrow() == content_type);
        button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
        button.set_tooltip_text(&kit::t(&format!("@content.{content_type}")).to_string());
        let selection_for = selection.clone();
        let name_for = content_type.clone();
        let buttons_for = type_buttons.clone();
        let button_for = button.clone();
        let callable = Callable::from_fn("content_type_pick", move |_args: &[&Variant]| {
            *selection_for.borrow_mut() = name_for.clone();
            let buttons = buttons_for.borrow();
            for other in buttons.iter() {
                let mut other = other.clone();
                if other == button_for {
                    other.set_pressed(true);
                } else {
                    other.set_pressed(false);
                }
            }
        });
        button.connect("pressed", &callable);
        type_buttons.borrow_mut().push(button.clone());
        type_row.add_child(&button);
    }
    box_.add_child(&type_row);
    accept.add_child(&box_);
    if let Some(mut ok) = accept.get_ok_button() {
        ok.set_visible(false);
    }
    let mut actions = factory::hbox(6);
    let cancel_target = accept.clone();
    let cancel_label = kit::t("@cancel").to_string();
    let mut cancel = factory::icon_button(
        "cancel",
        &cancel_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let callable = Callable::from_fn("content_add_cancel", move |_args: &[&Variant]| {
        let mut target = cancel_target.clone();
        target.queue_free();
    });
    cancel.connect("pressed", &callable);
    actions.add_child(&cancel);

    let file_target = accept.clone();
    let file_dialog = dialog.clone();
    let state_for_file = state.clone();
    let selection_for_file = selection.clone();
    let name_for_file = name_field.clone();
    let file_label = kit::t("@asset.content.import.file").to_string();
    let mut import_file = factory::icon_button(
        "file-text",
        &file_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let callable = Callable::from_fn("content_add_file", move |_args: &[&Variant]| {
        let chosen = selection_for_file.borrow().clone();
        let name = name_for_file.get_text().to_string();
        let mut target = file_target.clone();
        target.queue_free();
        let dialog_for = file_dialog.clone();
        let state_for = state_for_file.clone();
        let label = kit::t("@asset.content.import.file").to_string();
        map_assets_open_file(
            &file_dialog,
            &label,
            map_assets_text_filters(),
            false,
            Rc::new(move |paths: Vec<String>| {
                let path = paths.first().cloned().unwrap_or_default();
                let text = map_assets_read_text(&path);
                let result = editor_call(
                    "data_content_add",
                    &[
                        GString::from(name.as_str()).to_variant(),
                        GString::from(chosen.as_str()).to_variant(),
                        GString::from(text.as_str()).to_variant(),
                    ],
                );
                map_assets_apply(&dialog_for, &state_for, result);
            }),
        );
    });
    import_file.connect("pressed", &callable);
    actions.add_child(&import_file);

    let clip_target = accept.clone();
    let clip_dialog = dialog.clone();
    let state_for_clip = state.clone();
    let selection_for_clip = selection.clone();
    let name_for_clip = name_field.clone();
    let clip_label = kit::t("@asset.content.import.clipboard").to_string();
    let mut import_clipboard = factory::icon_button(
        "copy",
        &clip_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let callable = Callable::from_fn("content_add_clipboard", move |_args: &[&Variant]| {
        let chosen = selection_for_clip.borrow().clone();
        let name = name_for_clip.get_text().to_string();
        let text = DisplayServer::singleton().clipboard_get().to_string();
        let mut target = clip_target.clone();
        target.queue_free();
        let result = editor_call(
            "data_content_add",
            &[
                GString::from(name.as_str()).to_variant(),
                GString::from(chosen.as_str()).to_variant(),
                GString::from(text.as_str()).to_variant(),
            ],
        );
        map_assets_apply(&clip_dialog, &state_for_clip, result);
    });
    import_clipboard.connect("pressed", &callable);
    actions.add_child(&import_clipboard);
    accept.add_child(&actions);
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&accept);
    accept
        .popup_centered_ex()
        .minsize(Vector2i::new(500, 260))
        .done();
}
