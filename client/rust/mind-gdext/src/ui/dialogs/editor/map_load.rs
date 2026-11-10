// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map load dialog controller.
//! Upstream: editor/MapLoadDialog.java

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::editor::util_f::*;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindScroll};
use godot::builtin::Side;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, ButtonGroup, GridContainer, HBoxContainer, MarginContainer, PanelContainer,
    StyleBoxFlat, Texture2D, TextureRect,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "map_load" => Some(Box::new(MapLoadDialog::default())),
        _ => None,
    }
}

/// `table.defaults().size(250f, 90f).pad(4f)`.
const MAP_BUTTON_WIDTH: f32 = 250.0;
const MAP_BUTTON_HEIGHT: f32 = 90.0;
/// `new BorderImage(map.safeTexture(), 2f)` at `size(16 * 4f)`.
const MAP_PREVIEW_SIZE: f32 = 64.0;

/// The `map_load` view state.
#[derive(Default)]
struct MapLoadState {
    /// The picked map path (`_selected`).
    selected: String,
    /// The `@load` button, disabled until a map is picked.
    load_button: Option<Gd<Button>>,
    /// The `squareTogglet` radio group.
    group: Option<Gd<ButtonGroup>>,
}

/// `@editor.loadmap` — the map picker grid.
#[derive(Default)]
pub struct MapLoadDialog {
    state: Rc<RefCell<MapLoadState>>,
}

impl DialogController for MapLoadDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        map_load_ready(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        map_load_shown(dialog, &self.state);
    }
}

fn map_load_ready(dialog: &mut MindDialog, state: &Rc<RefCell<MapLoadState>>) {
    dialog.set_title_key("@editor.loadmap");
    // `makeButtonOverlay()` (`MapLoadDialog.java:34`): the padded title cell.
    dialog.apply_button_overlay(true);
    dialog.clear_buttons();
    state.borrow_mut().group = Some(ButtonGroup::new_gd());

    // `buttons.defaults().size(210f, 64f)`: Cancel (Icon.cancel), Load (Icon.ok).
    let hide = dialog.to_gd().callable("hide_dialog");
    let cancel_text = kit::t("@cancel").to_string();
    let mut cancel = dialog.add_button(&cancel_text, Some(hide), "cancel", 210.0);
    let mut size = cancel.get_custom_minimum_size();
    size.y = 64.0;
    cancel.set_custom_minimum_size(size);

    let dialog_gd = dialog.to_gd();
    let state_for_load = state.clone();
    let load = Callable::from_fn("map_load_selected", move |_args: &[&Variant]| {
        map_load_selected(&dialog_gd, &state_for_load);
    });
    let load_text = kit::t("@load").to_string();
    let mut load_button = dialog.add_button(&load_text, Some(load), "ok", 210.0);
    let mut size = load_button.get_custom_minimum_size();
    size.y = 64.0;
    load_button.set_custom_minimum_size(size);
    load_button.set_disabled(true);
    state.borrow_mut().load_button = Some(load_button);
}

fn map_load_shown(dialog: &mut MindDialog, state: &Rc<RefCell<MapLoadState>>) {
    {
        let mut state = state.borrow_mut();
        state.selected.clear();
        if let Some(mut load) = state.load_button.clone() {
            load.set_disabled(true);
        }
    }
    map_load_build(dialog, state);
}

/// `rebuild()` (`MapLoadDialog.java:35-71`).
fn map_load_build(dialog: &mut MindDialog, state: &Rc<RefCell<MapLoadState>>) {
    dialog.clear_content();
    let mut root = dialog.content_table();
    let maps = map_rows();
    if maps.is_empty() {
        let label = factory::label(&kit::t("@maps.none").to_string());
        root.bind_mut().add(&label);
        return;
    }
    let label = factory::label(&kit::t("@editor.selectmap").to_string());
    root.bind_mut().add(&label);
    root.bind_mut().row();

    // `ScrollPane(table)`, X scrolling off; `table.margin(10f)`.
    // code-instantiated: the map grid is data-driven (the map registry).
    let mut scroll = MindScroll::new_alloc();
    scroll.bind_mut().set_scrolling_disabled(true, false);
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    root.bind_mut().add(&scroll).grow_y();
    let mut margin = MarginContainer::new_alloc();
    for side in ["left", "top", "right", "bottom"] {
        margin.add_theme_constant_override(&format!("margin_{side}"), 10);
    }
    scroll.add_child(&margin);
    let mut grid = GridContainer::new_alloc();
    // `cols = max(width / 300, 1)`.
    let columns = ((viewport_width(dialog) / 300.0) as i32).max(1);
    grid.set_columns(columns);
    grid.add_theme_constant_override("h_separation", 8);
    grid.add_theme_constant_override("v_separation", 8);
    margin.add_child(&grid);
    for map in &maps {
        let button = map_load_button(state, map);
        grid.add_child(&button);
    }
    // The grid's full width (columns x 258 + margins) fixes the pane width.
    scroll.set_custom_minimum_size(Vector2::new(
        columns as f32 * (MAP_BUTTON_WIDTH + 8.0) + 12.0,
        0.0,
    ));
    root.bind_mut().row();
}

/// `maps.all()`: the registry rows, custom maps first.
fn map_rows() -> Vec<VarDictionary> {
    let Some(preview) = kit::node("/root/Spine/MindPreview") else {
        return Vec::new();
    };
    let mut preview = preview;
    if !preview.has_method("maps_list") {
        return Vec::new();
    }
    let value = preview.call("maps_list", &[]);
    let Ok(rows) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    let mut custom: Vec<VarDictionary> = Vec::new();
    let mut builtin: Vec<VarDictionary> = Vec::new();
    for row in rows.iter_shared() {
        let Ok(row) = row.try_to::<VarDictionary>() else {
            continue;
        };
        if dict_string(&row, "name").is_empty() {
            continue;
        }
        if dict_bool(&row, "custom", false) {
            custom.push(row);
        } else {
            builtin.push(row);
        }
    }
    custom.append(&mut builtin);
    custom
}

/// `_preview_texture(map_name)`.
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

/// One `squareTogglet` map button: preview (padLeft 5, 64 px) + name.
fn map_load_button(state: &Rc<RefCell<MapLoadState>>, map: &VarDictionary) -> Gd<Button> {
    // code-instantiated: one map card per registry row.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("squareTogglet");
    button.set_toggle_mode(true);
    if let Some(group) = state.borrow().group.clone() {
        button.set_button_group(&group);
    }
    button.set_custom_minimum_size(Vector2::new(MAP_BUTTON_WIDTH, MAP_BUTTON_HEIGHT));
    let path = dict_string(map, "path");
    let state_for_pick = state.clone();
    let path_for_press = path.clone();
    let callable = Callable::from_fn("map_pick", move |_args: &[&Variant]| {
        state_for_pick.borrow_mut().selected = path_for_press.clone();
        let disabled = path_for_press.is_empty();
        if let Some(mut load) = state_for_pick.borrow().load_button.clone() {
            load.set_disabled(disabled);
        }
    });
    button.connect("pressed", &callable);

    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, 8.0 + 5.0);
    row.set_offset(Side::RIGHT, -8.0);
    row.set_offset(Side::TOP, 8.0);
    row.set_offset(Side::BOTTOM, -8.0);
    row.add_theme_constant_override("separation", 5);
    row.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&row);

    // `BorderImage(texture, 2f)`: a 2 px `Pal.gray` frame around the preview.
    let mut frame = PanelContainer::new_alloc();
    // code-instantiated: the 2 px preview frame (a per-cell drawable).
    let mut border = StyleBoxFlat::new_gd();
    border.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
    border.set_border_color(Color::from_rgba8(0x45, 0x45, 0x45, 0xff));
    border.set_border_width_all(2);
    border.set_content_margin_all(2.0);
    frame.add_theme_stylebox_override("panel", &border);
    frame.set_custom_minimum_size(Vector2::new(MAP_PREVIEW_SIZE, MAP_PREVIEW_SIZE));
    frame.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    frame.set_mouse_filter(MouseFilter::IGNORE);
    let mut image = TextureRect::new_alloc();
    if let Some(texture) = map_preview_texture(&dict_string(map, "name")) {
        image.set_texture(&texture);
    }
    // `map.safeTexture()` keeps Arc's nearest filtering: crisp pixels.
    image.set_texture_filter(godot::classes::canvas_item::TextureFilter::NEAREST);
    image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
    image.set_mouse_filter(MouseFilter::IGNORE);
    frame.add_child(&image);
    row.add_child(&frame);

    let display = {
        let display = dict_string(map, "display_name");
        let name = dict_string(map, "name");
        if display.is_empty() { name } else { display }
    };
    let mut map_label = factory::label(&display);
    map_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
    map_label.set_horizontal_alignment(HorizontalAlignment::CENTER);
    map_label.set_vertical_alignment(VerticalAlignment::CENTER);
    map_label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    map_label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    map_label.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&map_label);
    button
}

/// `_load_selected`: `editor.begin_edit_map(path)` then hide.
fn map_load_selected(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapLoadState>>) {
    let selected = state.borrow().selected.clone();
    if selected.is_empty() {
        return;
    }
    let _ = editor_call(
        "begin_edit_map",
        &[GString::from(selected.as_str()).to_variant()],
    );
    if dialog.is_instance_valid() {
        let mut node = dialog.clone();
        node.bind_mut().close();
    }
}
