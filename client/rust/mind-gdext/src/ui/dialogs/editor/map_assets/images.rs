// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The images view of the map assets dialog.
//! Upstream: editor/data/MapAssetsDialog.java (`MapImagesView`)

use super::*;
use crate::gdutil::variant::{dict_bool_or, dict_i64_or, dict_text_or, to_bool_lenient};

pub(super) fn map_assets_build_images(
    dialog: &Gd<MindDialog>,
    list: &mut Gd<VBoxContainer>,
    state: &Rc<RefCell<MapAssetsState>>,
) {
    // code-instantiated: the image grid is record-driven; columns follow Java.
    let width = map_assets_viewport_width(dialog);
    let columns = (((width - 20.0) * 0.9 / (ASSET_IMAGE_CELL + 14.0)) as i32).max(1);
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(columns);
    grid.set_h_size_flags(SizeFlags::EXPAND_FILL);
    grid.add_theme_constant_override("h_separation", 4);
    grid.add_theme_constant_override("v_separation", 4);
    list.add_child(&grid);
    let mut count = 0;
    for row in &map_assets_rows_of(state, "image") {
        if dict_bool_or(row, "generated", false) {
            continue;
        }
        if !map_assets_matches(state, row, "path") {
            continue;
        }
        count += 1;
        let cell = map_assets_image_cell(dialog, state, row);
        grid.add_child(&cell);
    }
    if count == 0 {
        let mut empty = factory::label(&markup("@none.found"));
        empty.set_horizontal_alignment(HorizontalAlignment::CENTER);
        list.add_child(&empty);
    }
}

/// The image record fields the info page and the card need.
pub(super) struct AssetImageInfo {
    name: String,
    path: String,
    region: String,
    found: bool,
    width: i64,
    height: i64,
    size: i64,
}

pub(super) fn map_assets_image_cell(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
) -> Gd<PanelContainer> {
    let info = AssetImageInfo {
        name: dict_text_or(row, "name"),
        path: dict_text_or(row, "path"),
        region: dict_text_or(row, "region"),
        found: dict_bool_or(row, "found", false),
        width: dict_i64_or(row, "width", 1),
        height: dict_i64_or(row, "height", 1),
        size: dict_i64_or(row, "size", 0),
    };
    // code-instantiated: one `Styles.grayPanel` card per image record.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &flat_box(styles::GRAY_PANEL));
    panel.set_custom_minimum_size(Vector2::new(ASSET_IMAGE_CELL, ASSET_IMAGE_CELL));
    let mut box_ = factory::vbox(2);
    panel.add_child(&box_);

    // `t.add(new BorderImage(region, 4f)).scaling(Scaling.fit).size(size - 10f)`.
    if info.found {
        // code-instantiated: the preview texture is a runtime `dp-` region.
        let mut preview = TextureRect::new_alloc();
        preview.set_custom_minimum_size(Vector2::new(
            ASSET_IMAGE_CELL - 10.0,
            ASSET_IMAGE_CELL - 60.0,
        ));
        preview.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        preview.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
        if let Some(mut assets) = kit::assets()
            && let Some(texture) = assets
                .bind_mut()
                .find_region(GString::from(info.region.as_str()))
        {
            let texture: Gd<Texture2D> = texture.upcast();
            preview.set_texture(&texture);
        }
        box_.add_child(&preview);
    } else {
        // `Core.bundle.get("asset.image.packfailed")` placeholder cell.
        let mut missing = factory::label("⚠");
        missing.set_horizontal_alignment(HorizontalAlignment::CENTER);
        missing.set_vertical_alignment(VerticalAlignment::CENTER);
        missing.set_custom_minimum_size(Vector2::new(
            ASSET_IMAGE_CELL - 10.0,
            ASSET_IMAGE_CELL - 60.0,
        ));
        missing.set_tooltip_text(&kit::t("@asset.image.packfailed").to_string());
        box_.add_child(&missing);
    }

    let raw = if info.found {
        info.name.clone()
    } else {
        format!("[red]⚠[] {}", info.name)
    };
    let mut label = factory::label(&markup(&raw));
    // GDScript assigns `text_overrun_behavior`, which `RichTextLabel` does not
    // expose in Godot 4.7 (the assignment is a no-op there); the tooltip keeps
    // the full region/path.
    label.set_tooltip_text(&format!("{}\n{}", info.region, info.path));
    label.set_mouse_filter(MouseFilter::PASS);
    let dialog_for_label = dialog.clone();
    let info_for_label = Rc::new(AssetImageInfo {
        name: info.name.clone(),
        path: info.path.clone(),
        region: info.region.clone(),
        found: info.found,
        width: info.width,
        height: info.height,
        size: info.size,
    });
    let callable = Callable::from_fn("image_info_click", move |args: &[&Variant]| {
        let Some(event) = args
            .first()
            .and_then(|value| (*value).try_to::<Gd<godot::classes::InputEvent>>().ok())
        else {
            return;
        };
        let Ok(button) = event.try_cast::<godot::classes::InputEventMouseButton>() else {
            return;
        };
        if !button.is_pressed() {
            return;
        }
        map_assets_show_image_info(&dialog_for_label, &info_for_label);
    });
    label.connect("gui_input", &callable);
    box_.add_child(&label);
    let mut dims = factory::label(&format!("{}x{}", info.width, info.height));
    dims.add_theme_color_override("font_color", LIGHT_GRAY);
    box_.add_child(&dims);

    let mut actions = factory::hbox(0);
    let cell = (ASSET_IMAGE_CELL - 10.0) / 4.0;
    let dialog_for_pencil = dialog.clone();
    let state_for_pencil = state.clone();
    let path_for_pencil = info.path.clone();
    let pencil = Callable::from_fn("image_rename", move |_args: &[&Variant]| {
        if !dialog_for_pencil.is_instance_valid() {
            return;
        }
        let mut node = dialog_for_pencil.clone();
        let mut bound = node.bind_mut();
        map_assets_rename_image(&mut bound, &state_for_pencil, &path_for_pencil);
    });
    let mut pencil_button = factory::icon_button(
        "pencil",
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    pencil_button.set_custom_minimum_size(Vector2::new(cell, cell));
    pencil_button.set_tooltip_text(&kit::t("@save.rename").to_string());
    pencil_button.connect("pressed", &pencil);
    actions.add_child(&pencil_button);

    let dialog_for_info = dialog.clone();
    let info_for_button = Rc::new(AssetImageInfo {
        name: info.name.clone(),
        path: info.path.clone(),
        region: info.region.clone(),
        found: info.found,
        width: info.width,
        height: info.height,
        size: info.size,
    });
    let info_call = Callable::from_fn("image_info_button", move |_args: &[&Variant]| {
        map_assets_show_image_info(&dialog_for_info, &info_for_button);
    });
    let mut info_button = factory::icon_button(
        "info",
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    info_button.set_custom_minimum_size(Vector2::new(cell, cell));
    info_button.set_tooltip_text(&kit::t("@asset.image.info").to_string());
    info_button.connect("pressed", &info_call);
    actions.add_child(&info_button);

    let dialog_for_export = dialog.clone();
    let name_for_export = info.name.clone();
    let path_for_export = info.path.clone();
    let export = Callable::from_fn("image_export", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_export.clone();
        let name_for = name_for_export.clone();
        let path_for = path_for_export.clone();
        let label = kit::t("@editor.export").to_string();
        map_assets_save_file(
            &dialog_for,
            &label,
            &format!("{name_for}.png"),
            "*.png",
            Rc::new(move |target: String| {
                let exported = editor_call(
                    "data_asset_export",
                    &[
                        GString::from("image").to_variant(),
                        GString::from(path_for.as_str()).to_variant(),
                        GString::from(target.as_str()).to_variant(),
                    ],
                )
                .map(|value| to_bool_lenient(&value))
                .unwrap_or(false);
                if !exported {
                    map_assets_error(&kit::t("@asset.broken").to_string());
                }
            }),
        );
    });
    let mut export_button = factory::icon_button(
        "export",
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    export_button.set_custom_minimum_size(Vector2::new(cell, cell));
    export_button.set_tooltip_text(&kit::t("@export.file").to_string());
    export_button.connect("pressed", &export);
    actions.add_child(&export_button);

    let dialog_for_trash = dialog.clone();
    let state_for_trash = state.clone();
    let name_for_trash = info.name.clone();
    let trash = Callable::from_fn("image_delete", move |_args: &[&Variant]| {
        let message = kit::t("@asset.image.delete.confirm").to_string();
        map_assets_confirm(
            &dialog_for_trash,
            &state_for_trash,
            &message,
            PendingConfirm::ImageDelete(name_for_trash.clone()),
        );
    });
    let mut trash_button = factory::icon_button(
        "trash",
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    trash_button.set_custom_minimum_size(Vector2::new(cell, cell));
    trash_button.set_tooltip_text(&kit::t("@asset.image.delete.confirm").to_string());
    trash_button.connect("pressed", &trash);
    actions.add_child(&trash_button);
    box_.add_child(&actions);
    panel
}

pub(super) fn map_assets_show_image_info(dialog: &Gd<MindDialog>, info: &AssetImageInfo) {
    let _ = dialog;
    let env = info.path.contains("blocks/environment/");
    let mut lines: Vec<String> = Vec::new();
    if !info.found {
        lines.push(kit::t("@asset.image.packfailed").to_string());
        if env {
            lines.push(kit::t("@asset.image.packfailed.env").to_string());
        }
    }
    lines.push(kit::tm_format("@asset.image.name", &[&info.name]).to_string());
    lines.push(kit::tm_format("@asset.image.region.name", &[&info.region]).to_string());
    lines.push(kit::tm_format("@asset.image.path", &[&info.path]).to_string());
    lines.push(kit::tm_format("@asset.image.env", &[&env.to_string()]).to_string());
    lines.push(
        kit::tm_format(
            "@asset.image.size",
            &[&info.width.to_string(), &info.height.to_string()],
        )
        .to_string(),
    );
    lines.push(kit::tm_format("@asset.image.filesize", &[&info.size.to_string()]).to_string());
    let title = kit::t("@asset.image.info").to_string();
    map_assets_show_text(&title, &lines.join("\n"));
}

pub(super) fn map_assets_add_image(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let label = kit::t("@add").to_string();
    map_assets_open_file(
        dialog,
        &label,
        vec![
            String::from("*.png"),
            String::from("*.jpg"),
            String::from("*.jpeg"),
        ],
        true,
        Rc::new(move |paths: Vec<String>| {
            for path in paths {
                let result = editor_call(
                    "data_file_add",
                    &[
                        GString::from("image").to_variant(),
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

/// `MapImagesView` pencil (`ui.showTextInput("@save.rename", "@patch.path", ...)`).
pub(super) fn map_assets_rename_image(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapAssetsState>>,
    path: &str,
) {
    let dialog_gd = dialog.to_gd();
    let state_for = state.clone();
    let callable = Callable::from_fn("map_assets_image_rename", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let dialog_for = dialog_gd.clone();
        let state_for = state_for.clone();
        defer_action("map_assets_rename_result", move || {
            map_assets_on_image_rename(&dialog_for, &state_for, &text);
        });
    });
    connect_prompt_once(dialog, "map_assets_rename", "text_input_result", &callable);
    state.borrow_mut().renaming_image = String::from(path);
    let _ = ui_call(
        "show_text_input",
        &[
            kit::t("@save.rename").to_variant(),
            kit::t("@patch.path").to_variant(),
            1000i64.to_variant(),
            GString::from(path).to_variant(),
            false.to_variant(),
            false.to_variant(),
        ],
    );
}

pub(super) fn map_assets_on_image_rename(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    text: &str,
) {
    let old = std::mem::take(&mut state.borrow_mut().renaming_image);
    if old.is_empty() || text.is_empty() || text == old {
        return;
    }
    let result = editor_call(
        "data_image_rename",
        &[
            GString::from(old.as_str()).to_variant(),
            GString::from(text).to_variant(),
        ],
    );
    map_assets_apply(dialog, state, result);
}
