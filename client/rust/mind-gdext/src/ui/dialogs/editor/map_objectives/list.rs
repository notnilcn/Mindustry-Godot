// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Objective list, panel and picker wiring of the map objectives dialog.
//! Upstream: editor/MapObjectivesDialog.java

use godot::classes::Button;
use godot::classes::Control;
use godot::classes::DisplayServer;
use godot::classes::GridContainer;
use godot::classes::VBoxContainer;
use godot::classes::control::SizeFlags;
use godot::global::HorizontalAlignment;
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::factory;

use super::super::util_e::*;

use super::super::map_generate::{find_named, free_named};
use super::{
    OBJ_CANVAS_NODE, OBJ_COLOR_BOUND_META, OBJ_COLOR_FIELD_META, OBJ_CONFIRM_BOUND_META,
    OBJ_OBJECTIVES_META, OBJ_OVERLAY, OBJ_PENDING_CLASS_META, OBJ_PENDING_CLEAR_META,
    OBJ_PENDING_QUERY_META, OBJ_PICK_FIELD_META, OBJ_PICK_INDEX_META, OBJ_PICK_KIND_META,
    OBJ_PICKED_META, OBJ_PICKER_BOUND_META, OBJ_REQUEST_BUILD, OBJ_REQUEST_PANEL,
    OBJ_REQUEST_PICKED, OBJ_SELECTED_META,
};

/// `_objective_type_name(tag)`.
pub(super) fn objective_type_name(tag: &str) -> String {
    kit::t(&format!("@objective.{}.name", tag.to_lowercase())).to_string()
}

/// `ObjectiveMarker.typeName()`: `marker.<class>.name`.
pub(super) fn objective_marker_name(tag: &str) -> String {
    kit::t(&format!("@marker.{}.name", tag.to_lowercase())).to_string()
}

/// A 34 px icon button (`_small_button`).
pub(super) fn objective_small_button(icon: &str, handler: Callable) -> Gd<Button> {
    let mut button = factory::icon_button(
        icon,
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    button.set_custom_minimum_size(Vector2::new(34.0, 34.0));
    button.connect("pressed", &handler);
    button
}

/// `_is_visible_field(field)` (`FieldFlags`).
pub(super) fn objective_visible_field(field: &VarDictionary) -> bool {
    let flags = dict_array(field, "flags");
    !flags_has(&flags, "hidden") && !flags_has(&flags, "synthetic")
}

pub(super) fn flags_has(flags: &VarArray, name: &str) -> bool {
    flags.iter_shared().any(|value| {
        value
            .try_to::<GString>()
            .map(|value| value.to_string())
            .ok()
            .as_deref()
            == Some(name)
    })
}

/// The objective dictionary at `index` (`{}` when absent).
pub(super) fn objective_at(node: &Gd<MindDialog>, index: i64) -> VarDictionary {
    let objectives = meta_array(node, OBJ_OBJECTIVES_META);
    objectives
        .get(index.max(0) as usize)
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// `_default_of(field)`.
pub(super) fn objective_default_of(field: &VarDictionary) -> Variant {
    let kind = dict_text(field, "kind");
    if kind.starts_with("content:") {
        return GString::from("copper").to_variant();
    }
    match kind.as_str() {
        "team" => 1i64.to_variant(),
        "indexbool" => 1i64.to_variant(),
        "labelflag" => 3i64.to_variant(),
        "alignment" => 1i64.to_variant(),
        "texture" => {
            let mut value = VarDictionary::new();
            value.set("string", &GString::from("white").to_variant());
            value.to_variant()
        }
        "color" => GString::from("ffd37fff").to_variant(),
        "vec2i" => {
            let mut value = VarDictionary::new();
            value.set("x", &0i64.to_variant());
            value.set("y", &0i64.to_variant());
            value.to_variant()
        }
        "vec2f" => {
            let mut value = VarDictionary::new();
            value.set("x", &0.0f64.to_variant());
            value.set("y", &0.0f64.to_variant());
            value.to_variant()
        }
        "marker" => VarDictionary::new().to_variant(),
        "verts" => VarArray::new().to_variant(),
        _ => {
            if kind.starts_with("seq:") || kind.starts_with("map:") {
                return VarArray::new().to_variant();
            }
            Variant::nil()
        }
    }
}

/// Sets one field on one objective and writes the objectives tag.
pub(super) fn objective_set_field(
    node: &Gd<MindDialog>,
    index: i64,
    field_name: &str,
    value: Variant,
) {
    let mut objectives = meta_array(node, OBJ_OBJECTIVES_META);
    if index < 0 || index >= objectives.len() as i64 {
        return;
    }
    let Ok(mut objective) = objectives.at(index as usize).try_to::<VarDictionary>() else {
        return;
    };
    objective.set(field_name, &value);
    objectives.set(index as usize, &objective.to_variant());
    set_meta_array(&mut node.clone(), OBJ_OBJECTIVES_META, &objectives);
    objective_commit(node);
}

/// `_commit()`: writes the objectives JSON.
fn objective_commit(node: &Gd<MindDialog>) {
    let objectives = meta_array(node, OBJ_OBJECTIVES_META);
    let _ = editor_write(
        "set_objectives_json",
        &json_string(&objectives.to_variant()),
    );
}

/// `rebuildObjectives`: programmatic objectives (editorX == -999) get a grid
/// layout before the canvas draws them.
pub(super) fn objective_normalize_positions(node: &Gd<MindDialog>) {
    let mut objectives = meta_array(node, OBJ_OBJECTIVES_META);
    let default_pos = pack_point2(-999, -999);
    let unplaced = objectives.iter_shared().any(|value| {
        value
            .try_to::<VarDictionary>()
            .map(|objective| match objective.get("editorPos") {
                Some(position) => as_int(Some(position)) as i32 == default_pos,
                None => true,
            })
            .unwrap_or(true)
    });
    if !unplaced {
        return;
    }
    let width: i64 = 7;
    let total_width = objectives.len() as i64 * width;
    let mut columns = objectives.len() as i64;
    let mut rows = 1i64;
    if total_width > 100 {
        rows = total_width / 100;
        columns = 100 / width;
    }
    let mut index = 0usize;
    for y in 0..rows {
        for x in 0..columns {
            if index >= objectives.len() {
                break;
            }
            let Ok(mut objective) = objectives.at(index).try_to::<VarDictionary>() else {
                index += 1;
                continue;
            };
            objective.set(
                "editorPos",
                &i64::from(pack_point2((x * width) as i32, y as i32)).to_variant(),
            );
            objectives.set(index, &objective.to_variant());
            index += 1;
        }
    }
    if index > 0 {
        set_meta_array(&mut node.clone(), OBJ_OBJECTIVES_META, &objectives);
        objective_commit(node);
    }
}

/// `_move(direction)`.
pub(super) fn objective_move(node: &Gd<MindDialog>, direction: i64) {
    let selected = meta_int(node, OBJ_SELECTED_META, -1);
    let target = selected + direction;
    let mut objectives = meta_array(node, OBJ_OBJECTIVES_META);
    if selected < 0 || target < 0 || target >= objectives.len() as i64 {
        return;
    }
    let first = objectives.at(selected as usize);
    let second = objectives.at(target as usize);
    objectives.set(selected as usize, &second);
    objectives.set(target as usize, &first);
    let mut meta = node.clone();
    set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &objectives);
    set_meta_int(&mut meta, OBJ_SELECTED_META, target);
    objective_commit(node);
    request_refresh(node, OBJ_REQUEST_BUILD);
}

/// `_remove_selected()`.
pub(super) fn objective_remove_selected(node: &Gd<MindDialog>) {
    let selected = meta_int(node, OBJ_SELECTED_META, -1);
    let mut objectives = meta_array(node, OBJ_OBJECTIVES_META);
    if selected < 0 || selected >= objectives.len() as i64 {
        return;
    }
    objectives.remove(selected as usize);
    let mut meta = node.clone();
    set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &objectives);
    set_meta_int(&mut meta, OBJ_SELECTED_META, -1);
    objective_commit(node);
    request_refresh(node, OBJ_REQUEST_BUILD);
}

/// The `@add` objective type picker (`getProvider(MapObjective.class)`).
pub(super) fn objective_open_class_picker(node: &Gd<MindDialog>) {
    let classes = editor_array("objective_classes_json");
    if classes.is_empty() {
        return;
    }
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    // code-instantiated: per-action popup panels have no static scene.
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(3);
    grid.add_theme_constant_override("h_separation", 4);
    grid.add_theme_constant_override("v_separation", 4);
    for row in dict_rows(&classes) {
        let tag = dict_text(&row, "tag");
        let mut button = factory::button(&objective_type_name(&tag));
        button.set_theme_type_variation("flatt");
        button.set_text_alignment(HorizontalAlignment::LEFT);
        button.set_custom_minimum_size(Vector2::new(195.0, 56.0));
        let target = node.clone();
        let callable = Callable::from_fn("objective_pick_class", move |_args: &[&Variant]| {
            objective_pick_class(&target, &tag);
        });
        button.connect("pressed", &callable);
        grid.add_child(&button);
    }
    let title = kit::t("@add").to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        OBJ_OVERLAY,
        &title,
        640.0,
        380.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.8),
    ) {
        overlay.add_content(&grid);
    }
}

/// `_pick_class(tag, overlay)` then hide: append or replace the objective.
fn objective_pick_class(node: &Gd<MindDialog>, tag: &str) {
    free_named(node, OBJ_OVERLAY);
    let created = editor_json("objective_default_json", &[GString::from(tag).to_variant()])
        .map(|text| json_object(&text))
        .unwrap_or_default();
    if created.is_empty() {
        return;
    }
    let pending = meta_int(node, OBJ_PENDING_CLASS_META, -1);
    let mut objectives = meta_array(node, OBJ_OBJECTIVES_META);
    if pending >= 0 && pending < objectives.len() as i64 {
        // Keep the canvas position when changing class.
        let old = objectives
            .at(pending as usize)
            .try_to::<VarDictionary>()
            .unwrap_or_default();
        let mut created = created;
        if let Some(position) = old.get("editorPos") {
            created.set("editorPos", &position);
        }
        objectives.set(pending as usize, &created.to_variant());
        let mut meta = node.clone();
        set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &objectives);
        set_meta_int(&mut meta, OBJ_SELECTED_META, pending);
        set_meta_int(&mut meta, OBJ_PENDING_CLASS_META, -1);
        objective_commit(node);
        request_refresh(node, OBJ_REQUEST_BUILD);
        return;
    }
    set_meta_int(&mut node.clone(), OBJ_PENDING_CLASS_META, -1);
    // `@add` -> `canvas::query`: click the canvas to place it (`placeQuery`).
    set_meta_dict(&mut node.clone(), OBJ_PENDING_QUERY_META, &created);
    if let Some(mut canvas) = find_named(node, OBJ_CANVAS_NODE)
        && canvas.has_method("set_querying")
    {
        let _ = canvas.call("set_querying", &[true.to_variant()]);
    }
    ui_show_info(&kit::t("@editor.objectives"));
}

/// `@edit.menu`: copy/load/clear (`:496-522`).
pub(super) fn objective_show_edit_menu(node: &Gd<MindDialog>) {
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    let mut rows = VBoxContainer::new_alloc();
    let objectives = meta_array(node, OBJ_OBJECTIVES_META);
    let mut copy = factory::icon_button(
        "copy",
        &kit::t("@copy.clipboard").to_string(),
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    copy.set_disabled(objectives.is_empty());
    let target = node.clone();
    let callable = Callable::from_fn("objective_copy", move |_args: &[&Variant]| {
        let objectives = meta_array(&target, OBJ_OBJECTIVES_META);
        DisplayServer::singleton().clipboard_set(&json_string(&objectives.to_variant()));
        ui_show_info(&kit::t("@copied"));
        free_named(&target, OBJ_OVERLAY);
    });
    copy.connect("pressed", &callable);
    rows.add_child(&copy);
    let mut load = factory::icon_button(
        "download",
        &kit::t("@load.clipboard").to_string(),
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let target = node.clone();
    let callable = Callable::from_fn("objective_load", move |_args: &[&Variant]| {
        let text = DisplayServer::singleton().clipboard_get();
        let parsed = json_value(&text);
        let Ok(objectives) = parsed.try_to::<VarArray>() else {
            ui_show_info(&kit::t("@waves.invalid"));
            free_named(&target, OBJ_OVERLAY);
            return;
        };
        let mut meta = target.clone();
        set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &objectives);
        set_meta_int(&mut meta, OBJ_SELECTED_META, -1);
        objective_commit(&target);
        free_named(&target, OBJ_OVERLAY);
        request_refresh(&target, OBJ_REQUEST_BUILD);
    });
    load.connect("pressed", &callable);
    rows.add_child(&load);
    let mut clear = factory::icon_button(
        "cancel",
        &kit::t("@clear").to_string(),
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let target = node.clone();
    let callable = Callable::from_fn("objective_clear", move |_args: &[&Variant]| {
        set_meta_bool(&mut target.clone(), OBJ_PENDING_CLEAR_META, true);
        free_named(&target, OBJ_OVERLAY);
        ui_show_confirm(&format!(
            "{}\n{}",
            kit::t("@confirm"),
            kit::t("@settings.clear.confirm")
        ));
    });
    clear.connect("pressed", &callable);
    rows.add_child(&clear);
    let mut back = factory::button(&kit::t("@back").to_string());
    let target = node.clone();
    let callable = Callable::from_fn("objective_back", move |_args: &[&Variant]| {
        free_named(&target, OBJ_OVERLAY);
    });
    back.connect("pressed", &callable);
    rows.add_child(&back);
    let title = kit::t("@edit.menu").to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        OBJ_OVERLAY,
        &title,
        320.0,
        320.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.8),
    ) {
        overlay.add_content(&rows);
    }
}

/// `_connect_confirm()`: the `@clear` answer.
pub(super) fn objective_bind_confirm(node: &Gd<MindDialog>) {
    if node.has_meta(OBJ_CONFIRM_BOUND_META) {
        return;
    }
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_signal("confirm_result") {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("objective_confirm", move |args: &[&Variant]| {
        let confirmed = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        let mut meta = target.clone();
        if !meta_bool(&meta, OBJ_PENDING_CLEAR_META) {
            return;
        }
        set_meta_bool(&mut meta, OBJ_PENDING_CLEAR_META, false);
        if !confirmed {
            return;
        }
        set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &VarArray::new());
        set_meta_int(&mut meta, OBJ_SELECTED_META, -1);
        objective_commit(&target);
        request_refresh_deferred(&target, OBJ_REQUEST_BUILD);
    });
    ui.connect("confirm_result", &callable);
    node.clone()
        .set_meta(OBJ_CONFIRM_BOUND_META, &true.to_variant());
}

/// One byte of a `to_rgba32`-style hex string.
pub(super) fn hex_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The canvas (`MapObjectivesCanvas`) signal wiring (one bind per instance).
pub(super) fn objective_bind_canvas(node: &Gd<MindDialog>, canvas: &Gd<Control>) {
    let mut canvas = canvas.clone();
    if canvas.has_signal("node_selected") {
        let target = node.clone();
        let callable = Callable::from_fn("objective_selected", move |args: &[&Variant]| {
            let index = as_int(args.first().map(|value| (*value).clone()));
            set_meta_int(&mut target.clone(), OBJ_SELECTED_META, index);
            request_refresh(&target, OBJ_REQUEST_PANEL);
        });
        canvas.connect("node_selected", &callable);
    }
    if canvas.has_signal("node_moved") {
        let target = node.clone();
        let callable = Callable::from_fn("objective_moved", move |args: &[&Variant]| {
            let index = as_int(args.first().map(|value| (*value).clone()));
            let x = as_int(args.get(1).map(|value| (*value).clone())) as i32;
            let y = as_int(args.get(2).map(|value| (*value).clone())) as i32;
            objective_set_field(
                &target,
                index,
                "editorPos",
                i64::from(pack_point2(x, y)).to_variant(),
            );
        });
        canvas.connect("node_moved", &callable);
    }
    if canvas.has_signal("query_placed") {
        let target = node.clone();
        let callable = Callable::from_fn("objective_placed", move |args: &[&Variant]| {
            let x = as_int(args.first().map(|value| (*value).clone())) as i32;
            let y = as_int(args.get(1).map(|value| (*value).clone())) as i32;
            let mut pending = meta_dict(&target, OBJ_PENDING_QUERY_META);
            pending.set("editorPos", &i64::from(pack_point2(x, y)).to_variant());
            let mut objectives = meta_array(&target, OBJ_OBJECTIVES_META);
            objectives.push(&pending);
            let mut meta = target.clone();
            set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &objectives);
            set_meta_dict(&mut meta, OBJ_PENDING_QUERY_META, &VarDictionary::new());
            set_meta_int(&mut meta, OBJ_SELECTED_META, objectives.len() as i64 - 1);
            if let Some(mut canvas) = find_named(&target, OBJ_CANVAS_NODE)
                && canvas.has_method("set_querying")
            {
                let _ = canvas.call("set_querying", &[false.to_variant()]);
            }
            objective_commit(&target);
            request_refresh(&target, OBJ_REQUEST_BUILD);
        });
        canvas.connect("query_placed", &callable);
    }
}

/// Instantiates `res://scenes/editor/map_objectives_canvas.tscn`.
pub(super) fn instantiate_objectives_canvas() -> Option<Gd<Control>> {
    let scene = godot::tools::try_load::<godot::classes::PackedScene>(&GString::from(
        "res://scenes/editor/map_objectives_canvas.tscn",
    ))
    .ok()?;
    let node = scene.instantiate()?;
    let mut control = node.try_cast::<Control>().ok()?;
    control.set_name(OBJ_CANVAS_NODE);
    control.set_h_size_flags(SizeFlags::EXPAND_FILL);
    control.set_v_size_flags(SizeFlags::EXPAND_FILL);
    control.set_custom_minimum_size(Vector2::new(480.0, 320.0));
    Some(control)
}

/// `_connect_picker()` + `_open_list_picker`: the `list_select` flow.
pub(super) fn objective_bind_picker(node: &Gd<MindDialog>) {
    if node.has_meta(OBJ_PICKER_BOUND_META) {
        return;
    }
    let Some(mut picker) = dialog_node("list_select") else {
        return;
    };
    if !picker.has_signal("selected") {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("objective_picked", move |args: &[&Variant]| {
        let name = args
            .first()
            .map(|value| value.to::<GString>())
            .unwrap_or_default();
        set_meta_text(&mut target.clone(), OBJ_PICKED_META, &name.to_string());
        request_refresh_deferred(&target, OBJ_REQUEST_PICKED);
    });
    picker.connect("selected", &callable);
    node.clone()
        .set_meta(OBJ_PICKER_BOUND_META, &true.to_variant());
}

/// `_open_content_picker(content_type_name, setter)` via `list_select`.
pub(super) fn objective_open_content_picker(
    node: &Gd<MindDialog>,
    index: i64,
    field_name: &str,
    kind: &str,
) {
    let Some(mut host) = kit::sim_host() else {
        return;
    };
    if !host.has_method("content_list") {
        return;
    }
    let listed = host.call("content_list", &[GString::from(kind).to_variant()]);
    let Ok(items) = listed.try_to::<PackedStringArray>() else {
        return;
    };
    if items.is_empty() {
        return;
    }
    let names: Vec<String> = items
        .as_slice()
        .iter()
        .map(|value| value.to_string())
        .collect();
    let mut meta = node.clone();
    set_meta_int(&mut meta, OBJ_PICK_INDEX_META, index);
    set_meta_text(&mut meta, OBJ_PICK_FIELD_META, field_name);
    set_meta_text(&mut meta, OBJ_PICK_KIND_META, "content");
    objective_open_list_picker(node, "", &names);
}

/// `_open_team_picker(setter)`: the base-team picker (`palette_teams`).
pub(super) fn objective_open_team_picker(node: &Gd<MindDialog>, index: i64, field_name: &str) {
    let names = palette_team_names();
    let mut meta = node.clone();
    set_meta_int(&mut meta, OBJ_PICK_INDEX_META, index);
    set_meta_text(&mut meta, OBJ_PICK_FIELD_META, field_name);
    set_meta_text(&mut meta, OBJ_PICK_KIND_META, "team");
    objective_open_list_picker(node, "", &names);
}

/// `_open_list_picker(items, on_selected)`.
fn objective_open_list_picker(node: &Gd<MindDialog>, title_key: &str, items: &[String]) {
    if kit::ui().is_none() {
        return;
    }
    objective_bind_picker(node);
    let mut list = VarArray::new();
    for item in items {
        list.push(&GString::from(item.as_str()).to_variant());
    }
    let mut context = VarDictionary::new();
    context.set("title", &GString::from(title_key).to_variant());
    context.set("items", &list.to_variant());
    context.set("reset", &GString::from("").to_variant());
    ui_open_dialog("list_select", &context);
}

/// `_on_picked(selected_name)`: answers the pending content/team field.
pub(super) fn objective_apply_picked(node: &Gd<MindDialog>) {
    let name = meta_text(node, OBJ_PICKED_META);
    set_meta_text(&mut node.clone(), OBJ_PICKED_META, "");
    if name.is_empty() {
        return;
    }
    let index = meta_int(node, OBJ_PICK_INDEX_META, -1);
    let field_name = meta_text(node, OBJ_PICK_FIELD_META);
    let kind = meta_text(node, OBJ_PICK_KIND_META);
    let value = if kind == "team" {
        team_id_by_name(&name).to_variant()
    } else {
        GString::from(name.as_str()).to_variant()
    };
    objective_set_field(node, index, &field_name, value);
}

/// The `Color` interpreter's picker binding.
pub(super) fn objective_bind_color(node: &Gd<MindDialog>) {
    if node.has_meta(OBJ_COLOR_BOUND_META) {
        return;
    }
    let Some(mut picker) = dialog_node("picker") else {
        return;
    };
    if !picker.has_signal("color_selected") {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("objective_color", move |args: &[&Variant]| {
        let Some(value) = args.first() else {
            return;
        };
        let Ok(color) = value.try_to::<Color>() else {
            return;
        };
        let index = meta_int(&target, OBJ_SELECTED_META, -1);
        let field = meta_text(&target, OBJ_COLOR_FIELD_META);
        if index < 0 || field.is_empty() {
            return;
        }
        let hex = format!(
            "{:02x}{:02x}{:02x}{:02x}",
            hex_byte(color.r),
            hex_byte(color.g),
            hex_byte(color.b),
            hex_byte(color.a)
        );
        objective_set_field(
            &target,
            index,
            &field,
            GString::from(hex.as_str()).to_variant(),
        );
        request_refresh(&target, OBJ_REQUEST_PANEL);
    });
    picker.connect("color_selected", &callable);
    node.clone()
        .set_meta(OBJ_COLOR_BOUND_META, &true.to_variant());
}

/// `_open_color_picker(current, setter)`.
pub(super) fn objective_open_color_picker(node: &Gd<MindDialog>, current: &str, field_name: &str) {
    if kit::ui().is_none() {
        return;
    }
    if dialog_node("picker").is_none() {
        return;
    }
    set_meta_text(&mut node.clone(), OBJ_COLOR_FIELD_META, field_name);
    objective_bind_color(node);
    // `picker.set_color(...)` has no native equivalent: the seed colour rides
    // the `open_dialog` context (`{"color": "#rrggbbaa", "alpha": true}`), the
    // same seam `dialogs/batch_g.rs` documents for `ColorPicker.show`.
    let seed = if current.len() == 8 {
        current.to_string()
    } else {
        String::from("ffd37f")
    };
    let mut context = VarDictionary::new();
    context.set("color", &GString::from(seed.as_str()).to_variant());
    context.set("alpha", &true.to_variant());
    ui_open_dialog("picker", &context);
}

/// `_color_from_hex(hex)`.
pub(super) fn color_from_hex(hex: &str) -> Color {
    if hex.len() == 8 {
        let parse = |range: std::ops::Range<usize>| {
            hex.get(range)
                .and_then(|part| u8::from_str_radix(part, 16).ok())
                .unwrap_or(0)
        };
        return Color::from_rgba8(parse(0..2), parse(2..4), parse(4..6), parse(6..8));
    }
    styles::ACCENT
}
