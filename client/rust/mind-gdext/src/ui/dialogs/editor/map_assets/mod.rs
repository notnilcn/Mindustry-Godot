// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map assets dialog controller; this directory holds its submodules.
//! Upstream: editor/data/MapAssetsDialog.java

use crate::gdutil::variant::to_bool_lenient;
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::editor::util::*;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindLabel, MindScroll, styles};
use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::file_dialog::{Access, FileMode};
use godot::classes::{
    AcceptDialog, Button, ColorRect, Control, DisplayServer, FileAccess, FileDialog, GridContainer,
    HBoxContainer, Json, LineEdit, Os, PanelContainer, Texture2D, TextureRect, VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub mod audio;
pub mod bundles;
pub mod content;
pub mod helpers;
pub mod images;
pub mod patches;

use self::audio::*;
use self::bundles::*;
use self::content::*;
use self::helpers::*;
use self::images::*;
use self::patches::*;

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "map_assets" => Some(Box::new(MapAssetsDialog::default())),
        _ => None,
    }
}

/// `DataAssetType.all` in ordinal order (the tab identity).
const ASSET_TYPES: [&str; 6] = ["patch", "content", "bundle", "image", "sound", "music"];

/// `MapAssetsDialog` row geometry: `h = 50f`, `mobile ? 390f : 450f`.
const ASSET_ROW_HEIGHT: f32 = 50.0;

const ASSET_ROW_WIDTH: f32 = 450.0;

const ASSET_ROW_WIDTH_MOBILE: f32 = 390.0;

const ASSET_IMAGE_CELL: f32 = 200.0;

/// `iconMed` (32 px) in the icon-only cells.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
const ASSET_ICON_SIZE: i32 = 32;

/// `Vars.patchesGuideURL`.
const PATCHES_GUIDE_URL: &str = "https://mindustrygame.github.io/wiki/datapatches/";

/// `Color.lightGray` (Godot `0.62` grey).
const LIGHT_GRAY: Color = Color::from_rgba(0.62, 0.62, 0.62, 1.0);

/// `DataAssetType.localized()` bundle keys.
fn asset_type_key(kind: &str) -> &'static str {
    match kind {
        "patch" => "@asset.patch",
        "content" => "@asset.content",
        "bundle" => "@asset.bundle",
        "image" => "@asset.image",
        "sound" => "@asset.sound",
        "music" => "@asset.music",
        _ => "",
    }
}

/// `MapAssetsDialog.typeIcons`.
fn asset_type_icon(kind: &str) -> &'static str {
    match kind {
        "patch" => "file-code",
        "content" => "box",
        "bundle" => "file-text",
        "image" => "image",
        "sound" => "volume-up",
        "music" => "music",
        _ => "file-text",
    }
}

/// `MapContentView.contentIcons` (`drizzle` has no alias, `effect` stands in).
fn asset_content_icon(content_type: &str) -> &'static str {
    match content_type {
        "item" => "box",
        "liquid" => "liquid",
        "unit" => "units",
        "block" => "distribution",
        "planet" => "planet",
        "weather" => "effect",
        "status" => "power",
        _ => "box",
    }
}

/// `ContentAsset.loadableContent` (the add-content type row).
const ASSET_CONTENT_TYPES: [&str; 6] = ["item", "block", "liquid", "status", "unit", "weather"];

/// A deferred confirmation target (`ui.showConfirm` answer).
#[derive(Clone)]
enum PendingConfirm {
    ClearAll,
    PatchDelete(i64),
    ContentDelete(String),
    BundleDelete(String),
    ImageDelete(String),
    AudioDelete(String, String),
}

/// A pending file-chooser result (`_open_file`) handler.
type AssetPathsHandler = Rc<dyn Fn(Vec<String>)>;

/// A pending save-chooser result (`_save_file`) handler.
type AssetTextHandler = Rc<dyn Fn(String)>;

/// An import-source result (`_import_choices`) handler.
type AssetImportHandler = Rc<dyn Fn(String)>;

/// The `map_assets` view state.
#[derive(Default)]
struct MapAssetsState {
    /// `_type`: the active `DataAssetType` (`patch` by default).
    kind: String,
    search: String,
    rows: Vec<VarDictionary>,
    type_buttons: Vec<(String, Gd<Button>)>,
    type_label: Option<Gd<MindLabel>>,
    search_field: Option<Gd<LineEdit>>,
    list: Option<Gd<VBoxContainer>>,
    /// The one pending `confirm_result` handler.
    pending_confirm: Option<PendingConfirm>,
    /// Last playing data audio row name (music pause icon).
    playing_audio: String,
    /// Image path awaiting the rename prompt result.
    renaming_image: String,
}

/// `@asset.patch` — the data-asset manager (five upstream view classes).
#[derive(Default)]
pub struct MapAssetsDialog {
    state: Rc<RefCell<MapAssetsState>>,
}

impl DialogController for MapAssetsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        map_assets_ready(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        map_assets_shown(dialog, &self.state);
    }
}

fn map_assets_ready(dialog: &mut MindDialog, state: &Rc<RefCell<MapAssetsState>>) {
    // `super("")`: the title is not displayed.
    dialog.set_title_literal("");
    dialog.should_pause = false;
    dialog.apply_cont_top(true);
    {
        let mut state = state.borrow_mut();
        if state.kind.is_empty() {
            state.kind = String::from("patch");
        }
    }
    let dialog_gd = dialog.to_gd();
    let state_for_confirm = state.clone();
    let callable = Callable::from_fn("map_assets_confirm", move |args: &[&Variant]| {
        let confirmed = args
            .first()
            .map(|value| to_bool_lenient(value))
            .unwrap_or(false);
        let dialog_for = dialog_gd.clone();
        let state_for = state_for_confirm.clone();
        defer_action("map_assets_confirm_result", move || {
            map_assets_on_confirm(&dialog_for, &state_for, confirmed);
        });
    });
    connect_prompt_once(dialog, "map_assets_confirm", "confirm_result", &callable);
}

/// `shown`: reset search, then `changeType(patch)`.
fn map_assets_shown(dialog: &mut MindDialog, state: &Rc<RefCell<MapAssetsState>>) {
    {
        let mut state = state.borrow_mut();
        state.search.clear();
        if let Some(mut field) = state.search_field.clone() {
            field.set_text("");
        }
    }
    map_assets_refetch(state);
    map_assets_build(dialog, state);
}

/// Pulls the current record set from the owner (`state.data`).
fn map_assets_refetch(state: &Rc<RefCell<MapAssetsState>>) {
    let rows = editor_call("assets", &[])
        .and_then(|value| value.try_to::<VarArray>().ok())
        .map(|array| {
            array
                .iter_shared()
                .filter_map(|row| row.try_to::<VarDictionary>().ok())
                .collect()
        })
        .unwrap_or_default();
    state.borrow_mut().rows = rows;
}

fn map_assets_build(dialog: &mut MindDialog, state: &Rc<RefCell<MapAssetsState>>) {
    dialog.clear_content();
    let mut root = dialog.content_table();
    let dialog_gd = dialog.to_gd();
    let kind = state.borrow().kind.clone();

    // `cont.add(types).growX().left().row()`.
    let mut types = factory::hbox(4);
    types.set_h_size_flags(SizeFlags::EXPAND_FILL);
    root.bind_mut().add(&types).grow_x();
    root.bind_mut().row();

    // `types.button(Icon.menu, Styles.graySquarei, this::showEditMenu)`.
    let mut menu = factory::icon_button(
        "menu",
        "",
        "graySquarei",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    menu.set_custom_minimum_size(Vector2::new(50.0, 50.0));
    menu.set_tooltip_text(&kit::t("@edit.menu").to_string());
    let menu_gd = dialog_gd.clone();
    let menu_state = state.clone();
    let callable = Callable::from_fn("map_assets_edit_menu", move |_args: &[&Variant]| {
        map_assets_show_edit_menu(&menu_gd, &menu_state);
    });
    menu.connect("pressed", &callable);
    types.add_child(&menu);
    types.add_child(&map_assets_divider());

    state.borrow_mut().type_buttons.clear();
    for type_name in ASSET_TYPES {
        // `types.button(typeIcons[type.ordinal()], Styles.grayTogglei, ...)`.
        let mut button = factory::glyph_button(
            asset_type_icon(type_name),
            &kit::t(asset_type_key(type_name)).to_string(),
            "grayTogglei",
        );
        button.set_toggle_mode(true);
        button.set_pressed(kind == type_name);
        button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
        button.set_tooltip_text(&kit::t(asset_type_key(type_name)).to_string());
        let dialog_for = dialog_gd.clone();
        let state_for = state.clone();
        let name_for_switch = String::from(type_name);
        let callable = Callable::from_fn("map_assets_type", move |_args: &[&Variant]| {
            if !dialog_for.is_instance_valid() {
                return;
            }
            let mut node = dialog_for.clone();
            let mut bound = node.bind_mut();
            map_assets_change_type(&mut bound, &state_for, &name_for_switch);
        });
        button.connect("pressed", &callable);
        types.add_child(&button);
        state
            .borrow_mut()
            .type_buttons
            .push((String::from(type_name), button));
    }
    types.add_child(&map_assets_divider());

    // `types.table(Styles.grayPanel, t -> t.label(() -> currentType.localized()))`.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &flat_box(styles::GRAY_PANEL));
    panel.set_custom_minimum_size(Vector2::new(0.0, 50.0));
    let mut type_label = factory::label(&kit::t(asset_type_key(&kind)).to_string());
    type_label.set_horizontal_alignment(HorizontalAlignment::CENTER);
    panel.add_child(&type_label);
    types.add_child(&panel);
    state.borrow_mut().type_label = Some(type_label);

    let mut spacer = factory::space(0.0, 0.0);
    spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
    types.add_child(&spacer);

    // `types.button("@asset.guide", Icon.link, Styles.grayt, ...).size(200, 50)`.
    let mut guide = factory::icon_button(
        "link",
        &kit::t("@asset.guide").to_string(),
        "grayt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    guide.set_custom_minimum_size(Vector2::new(200.0, 50.0));
    let callable = Callable::from_fn("map_assets_guide", |_args: &[&Variant]| {
        let _ = Os::singleton().shell_open(PATCHES_GUIDE_URL);
    });
    guide.connect("pressed", &callable);
    types.add_child(&guide);

    // `cont.table(search -> image + field).growX().row()`.
    let mut search = factory::hbox(4);
    search.set_h_size_flags(SizeFlags::EXPAND_FILL);
    root.bind_mut().add(&search).grow_x();
    root.bind_mut().row();
    let mut zoom = factory::image("zoom");
    zoom.set_custom_minimum_size(Vector2::new(30.0, 30.0));
    zoom.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    search.add_child(&zoom);
    let mut field = factory::field(&kit::t("@search").to_string());
    field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    field.set_text(&GString::from(state.borrow().search.as_str()));
    let dialog_for_search = dialog_gd.clone();
    let state_for_search = state.clone();
    let callable = Callable::from_fn("map_assets_search", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        map_assets_on_search(&dialog_for_search, &state_for_search, &text);
    });
    field.connect("text_changed", &callable);
    search.add_child(&field);
    state.borrow_mut().search_field = Some(field);

    // `cont.pane(t -> list = t).grow().top()`.
    // code-instantiated: the record list is data-driven (`rebuild`).
    let mut scroll = MindScroll::new_alloc();
    scroll.bind_mut().set_scrolling_disabled(true, false);
    scroll.set_h_size_flags(SizeFlags::EXPAND_FILL);
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    scroll.set_custom_minimum_size(Vector2::new(0.0, 300.0));
    root.bind_mut().add(&scroll).grow_x().grow_y();
    root.bind_mut().row();
    let mut list = VBoxContainer::new_alloc();
    list.set_h_size_flags(SizeFlags::EXPAND_FILL);
    list.add_theme_constant_override("separation", 4);
    scroll.add_child(&list);
    state.borrow_mut().list = Some(list);

    map_assets_change_type(dialog, state, &kind);
}

/// `changeType(type)`: swap the action buttons, then `rebuild()`.
fn map_assets_change_type(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapAssetsState>>,
    kind: &str,
) {
    state.borrow_mut().kind = String::from(kind);
    if let Some(mut label) = state.borrow().type_label.clone() {
        label.set_text(&kit::t(asset_type_key(kind)));
    }
    let buttons = state.borrow().type_buttons.clone();
    for (key, mut button) in buttons {
        button.set_pressed(key == kind);
    }

    dialog.clear_buttons();
    let dialog_gd = dialog.to_gd();
    match kind {
        "patch" => {
            let for_add = dialog_gd.clone();
            let state_for = state.clone();
            let add = Callable::from_fn("map_assets_add_patch", move |_args: &[&Variant]| {
                map_assets_add_patch(&for_add, &state_for);
            });
            let text = kit::t("@add").to_string();
            dialog.add_button(&text, Some(add), "add", 190.0);
        }
        "content" => {
            let for_add = dialog_gd.clone();
            let state_for = state.clone();
            let add = Callable::from_fn("map_assets_add_content", move |_args: &[&Variant]| {
                map_assets_add_content(&for_add, &state_for);
            });
            let text = kit::t("@add").to_string();
            dialog.add_button(&text, Some(add), "add", 190.0);
        }
        "bundle" => {
            let for_add = dialog_gd.clone();
            let state_for = state.clone();
            let add = Callable::from_fn("map_assets_add_bundle", move |_args: &[&Variant]| {
                map_assets_add_bundle(&for_add, &state_for);
            });
            let text = kit::t("@add").to_string();
            dialog.add_button(&text, Some(add), "add", 190.0);
        }
        "image" => {
            let for_add = dialog_gd.clone();
            let state_for = state.clone();
            let add = Callable::from_fn("map_assets_add_image", move |_args: &[&Variant]| {
                map_assets_add_image(&for_add, &state_for);
            });
            let text = kit::t("@add").to_string();
            dialog.add_button(&text, Some(add), "add", 190.0);
            let for_menu = dialog_gd.clone();
            let state_for = state.clone();
            let edit = Callable::from_fn("map_assets_image_menu", move |_args: &[&Variant]| {
                map_assets_show_edit_menu(&for_menu, &state_for);
            });
            let text = kit::t("@edit").to_string();
            dialog.add_button(&text, Some(edit), "menu", 190.0);
        }
        "sound" | "music" => {
            let for_add = dialog_gd.clone();
            let state_for = state.clone();
            let kind_for_add = String::from(kind);
            let add = Callable::from_fn("map_assets_add_audio", move |_args: &[&Variant]| {
                map_assets_add_audio(&for_add, &state_for, &kind_for_add);
            });
            let text = kit::t("@add").to_string();
            dialog.add_button(&text, Some(add), "add", 190.0);
        }
        _ => {}
    }
    dialog.add_default_close_button();

    // Switching to the content view reloads content + sprites (`changeType`).
    if kind == "content" {
        let _ = editor_call("data_reload_content", &[false.to_variant()]);
        map_assets_refetch(state);
    }
    map_assets_rebuild(&dialog_gd, state);
}

/// `rebuild()`: clear the pane and run the current `AssetView.build`.
fn map_assets_rebuild(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    let list = state.borrow().list.clone();
    let Some(mut list) = list else {
        return;
    };
    kit::clear_children(&mut list);
    let kind = state.borrow().kind.clone();
    match kind.as_str() {
        "patch" => map_assets_build_patches(dialog, &mut list, state),
        "content" => map_assets_build_content(dialog, &mut list, state),
        "bundle" => map_assets_build_bundles(dialog, &mut list, state),
        "image" => map_assets_build_images(dialog, &mut list, state),
        "sound" | "music" => map_assets_build_audio(dialog, &mut list, state, &kind),
        _ => {}
    }
}

fn map_assets_on_search(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>, text: &str) {
    // `t.length() > 0 ? t.toLowerCase() : null`.
    state.borrow_mut().search = if text.is_empty() {
        String::new()
    } else {
        text.to_lowercase()
    };
    map_assets_rebuild(dialog, state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_type_tables_are_the_abi() {
        assert_eq!(ASSET_TYPES.len(), 6);
        assert_eq!(asset_type_key("patch"), "@asset.patch");
        assert_eq!(asset_type_icon("music"), "music");
        assert_eq!(asset_content_icon("weather"), "effect");
        assert_eq!(ASSET_CONTENT_TYPES.len(), 6);
    }
}
