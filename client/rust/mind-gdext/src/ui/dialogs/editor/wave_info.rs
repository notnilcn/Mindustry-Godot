// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The wave info dialog controller.
//! Upstream: editor/WaveInfoDialog.java

use godot::classes::Button;
use godot::classes::Control;
use godot::classes::DisplayServer;
use godot::classes::HBoxContainer;
use godot::classes::Label;
use godot::classes::LineEdit;
use godot::classes::PanelContainer;
use godot::classes::TextureRect;
use godot::classes::VBoxContainer;
use godot::classes::control::LayoutPreset;
use godot::classes::control::MouseFilter;
use godot::classes::control::SizeFlags;
use godot::classes::texture_rect::ExpandMode;
use godot::classes::texture_rect::StretchMode;
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::DialogController;
use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::MindScroll;
use crate::ui::kit::MindTable;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::factory;

use super::util_e::*;

use super::map_generate::find_named;

// ---------------------------------------------------------------------------
// WaveInfoDialog (`wave_info`, `WaveInfoDialog.java`)
// ---------------------------------------------------------------------------

/// `SpawnGroup.NEVER` (`i32::MAX`).
const WAVE_NEVER: i64 = 2_147_483_647;
/// `WaveInfoDialog.setup`: the left editor column.
const WAVE_PANEL_WIDTH: f32 = 390.0;
/// `spawner.getSpawns()` display cap (`i >= 20`).
const WAVE_SPAWN_CAP: usize = 20;

/// Working group list (`WaveInfoDialog.groups`).
const WAVE_GROUPS_META: &str = "mind_wave_groups";
/// Expanded group index (`expandedGroup`), -1 for none.
const WAVE_EXPANDED_META: &str = "mind_wave_expanded";
/// 0-based wave search (`search`), -1 for none.
const WAVE_SEARCH_META: &str = "mind_wave_search";
/// Unit-type filter (`filterType`), "" for none.
const WAVE_FILTER_META: &str = "mind_wave_filter";
/// `Sort` enum (`0` begin, `1` health, `2` type).
const WAVE_SORT_META: &str = "mind_wave_sort";
const WAVE_REVERSE_META: &str = "mind_wave_reverse";
/// Pending `list_select` target and the picked value.
const WAVE_PICKER_META: &str = "mind_wave_picker";
const WAVE_PICKED_META: &str = "mind_wave_picked";
const WAVE_PICKER_BOUND_META: &str = "mind_wave_picker_bound";
/// Inline panels (`@edit.menu` / `@waves.sort`).
const WAVE_EDIT_OPEN_META: &str = "mind_wave_edit_open";
const WAVE_SORT_OPEN_META: &str = "mind_wave_sort_open";
/// The graph node name (`WaveGraph` scene instance).
const WAVE_GRAPH_NODE: &str = "WaveGraph";

const WAVE_REQUEST_LIST: &str = "list";
const WAVE_REQUEST_PICKED: &str = "picked";
const WAVE_REQUEST_BUILD: &str = "build";

/// `@waves.title` — the wave group editor plus the `WaveGraph`.
#[derive(Default)]
pub struct WaveInfoDialog {
    /// The group list column (`_list`).
    list: Option<Gd<MindTable>>,
}

impl DialogController for WaveInfoDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@waves.title");
        dialog.should_pause = false;
        dialog.clear_buttons();
        // `addCloseButton()` first, then `@edit.menu` and `waves.random`.
        dialog.add_close_button(210.0);
        let node = dialog.to_gd();
        let edit = kit::t("@edit.menu").to_string();
        let edit_target = node.clone();
        let callable = Callable::from_fn("wave_edit_menu", move |_args: &[&Variant]| {
            let mut meta = edit_target.clone();
            let open = !meta_bool(&meta, WAVE_EDIT_OPEN_META);
            set_meta_bool(&mut meta, WAVE_EDIT_OPEN_META, open);
            set_meta_bool(&mut meta, WAVE_SORT_OPEN_META, false);
            request_refresh(&edit_target, WAVE_REQUEST_BUILD);
        });
        dialog.add_button(&edit, Some(callable), "edit", 250.0);
        let random = kit::t("@waves.random").to_string();
        let random_target = node.clone();
        let callable = Callable::from_fn("wave_random", move |_args: &[&Variant]| {
            if as_bool(editor_call("randomize_waves", &[])) {
                let groups = wave_read_groups();
                let mut meta = random_target.clone();
                set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
                set_meta_int(&mut meta, WAVE_EXPANDED_META, -1);
                request_refresh(&random_target, WAVE_REQUEST_BUILD);
            }
        });
        dialog.add_button(&random, Some(callable), "refresh", 200.0);
        wave_bind_picker(&node);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let request = take_request(dialog);
        match request.as_str() {
            WAVE_REQUEST_LIST => self.rebuild(dialog),
            WAVE_REQUEST_PICKED => {
                wave_apply_picked(&dialog.to_gd());
                self.rebuild(dialog);
            }
            WAVE_REQUEST_BUILD => self.build(dialog),
            // `shown()`: read the groups (vanilla fallback included) and build.
            _ => self.setup(dialog),
        }
    }
}

impl WaveInfoDialog {
    /// `shown()`: `_groups = _read_groups(); _expanded = -1; _commit(); _build()`.
    fn setup(&mut self, dialog: &mut MindDialog) {
        let node = dialog.to_gd();
        let groups = wave_read_groups();
        let mut meta = node.clone();
        set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
        set_meta_int(&mut meta, WAVE_EXPANDED_META, -1);
        set_meta_int(&mut meta, WAVE_SEARCH_META, -1);
        wave_commit(&node);
        self.build(dialog);
    }

    /// `_build()`: the search/list/add column and the wave graph.
    fn build(&mut self, dialog: &mut MindDialog) {
        if dialog.content().is_none() {
            return;
        }
        dialog.clear_content();
        let mut root = dialog.content_table();
        let node = dialog.to_gd();
        let mut row = HBoxContainer::new_alloc();
        row.set_h_size_flags(SizeFlags::EXPAND_FILL);
        row.set_v_size_flags(SizeFlags::EXPAND_FILL);
        root.bind_mut().add(&row).grow_x().grow_y();
        let mut left = VBoxContainer::new_alloc();
        left.set_custom_minimum_size(Vector2::new(WAVE_PANEL_WIDTH, 0.0));
        left.set_h_size_flags(SizeFlags::FILL);
        left.set_v_size_flags(SizeFlags::EXPAND_FILL);
        row.add_child(&left);
        // The search row (`@waves.search` + the unit filter button).
        let mut search = HBoxContainer::new_alloc();
        search.add_child(&factory::glyph_label("zoom", 20, Color::WHITE));
        let mut search_field = factory::field(&kit::t("@waves.search").to_string());
        search_field.set_max_length(8);
        search_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let search_target = node.clone();
        let callable = Callable::from_fn("wave_search", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| value.to::<GString>().to_string())
                .unwrap_or_default();
            let search = valid_int(&text).map(|value| value - 1).unwrap_or(-1);
            set_meta_int(&mut search_target.clone(), WAVE_SEARCH_META, search);
            request_refresh(&search_target, WAVE_REQUEST_LIST);
        });
        search_field.connect("text_changed", &callable);
        search.add_child(&search_field);
        let mut filter_button = factory::icon_button(
            "filter",
            "",
            "emptyi",
            godot::classes::box_container::AlignmentMode::BEGIN,
        );
        filter_button.set_custom_minimum_size(Vector2::new(46.0, 46.0));
        filter_button.set_tooltip_text(&kit::t("@waves.filter"));
        let filter_target = node.clone();
        let callable = Callable::from_fn("wave_filter", move |_args: &[&Variant]| {
            wave_open_picker(
                &filter_target,
                "@waves.filter",
                &wave_unit_names(),
                "@settings.resetKey",
                "filter",
            );
        });
        filter_button.connect("pressed", &callable);
        search.add_child(&filter_button);
        left.add_child(&search);
        // The scrolling group list (`pane(p -> ...)`).
        // code-instantiated: the group list is the runtime spawns table.
        let mut scroll = MindScroll::new_alloc();
        scroll.bind_mut().set_scrolling_disabled(true, false);
        let mut scroll_control = as_control(&scroll);
        scroll_control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        scroll_control.set_v_size_flags(SizeFlags::EXPAND_FILL);
        scroll_control.set_custom_minimum_size(Vector2::new(WAVE_PANEL_WIDTH - 20.0, 260.0));
        let mut list = scroll.bind_mut().content();
        list.bind_mut().grow_horizontal();
        self.list = Some(list);
        left.add_child(&scroll_control);
        // The add + sort action row.
        let mut actions = HBoxContainer::new_alloc();
        let mut add = factory::button(&kit::t("@add").to_string());
        add.set_custom_minimum_size(Vector2::new(0.0, 70.0));
        add.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let add_target = node.clone();
        let callable = Callable::from_fn("wave_add", move |_args: &[&Variant]| {
            wave_open_picker(&add_target, "", &wave_unit_names(), "", "add");
        });
        add.connect("pressed", &callable);
        actions.add_child(&add);
        let mut sort_button = factory::icon_button(
            "filter",
            "",
            "defaulti",
            godot::classes::box_container::AlignmentMode::BEGIN,
        );
        sort_button.set_custom_minimum_size(Vector2::new(64.0, 70.0));
        for (slot, region) in [
            ("normal", "button"),
            ("hover", "button-over"),
            ("pressed", "button-down"),
        ] {
            sort_button.add_theme_stylebox_override(slot, &button_nine_patch(region));
        }
        actions.add_theme_constant_override("separation", 6);
        let sort_target = node.clone();
        let callable = Callable::from_fn("wave_sort_open", move |_args: &[&Variant]| {
            let mut meta = sort_target.clone();
            let open = !meta_bool(&meta, WAVE_SORT_OPEN_META);
            set_meta_bool(&mut meta, WAVE_SORT_OPEN_META, open);
            set_meta_bool(&mut meta, WAVE_EDIT_OPEN_META, false);
            request_refresh(&sort_target, WAVE_REQUEST_BUILD);
        });
        sort_button.connect("pressed", &callable);
        actions.add_child(&sort_button);
        left.add_child(&actions);
        // The inline `@edit.menu` / `@waves.sort` panels.
        let mut edit_panel = wave_build_edit_panel(&node);
        edit_panel.set_visible(meta_bool(&node, WAVE_EDIT_OPEN_META));
        left.add_child(&edit_panel);
        let mut sort_panel = wave_build_sort_panel(&node);
        sort_panel.set_visible(meta_bool(&node, WAVE_SORT_OPEN_META));
        left.add_child(&sort_panel);
        // `cont.add(graph)`.
        if let Some(graph) = instantiate_wave_graph() {
            row.add_child(&graph);
        }
        self.rebuild(dialog);
    }

    /// `buildGroups`: sorts, filters and rebuilds every group card.
    fn rebuild(&mut self, dialog: &mut MindDialog) {
        let Some(mut list) = self.list.clone() else {
            return;
        };
        list.bind_mut().clear();
        let node = dialog.to_gd();
        wave_sort_groups(&node);
        let groups = meta_array(&node, WAVE_GROUPS_META);
        let search = meta_int(&node, WAVE_SEARCH_META, -1);
        let filter_type = meta_text(&node, WAVE_FILTER_META);
        let expanded = meta_int(&node, WAVE_EXPANDED_META, -1);
        let mut any = false;
        for index in 0..groups.len() {
            let Ok(group) = groups.at(index).try_to::<VarDictionary>() else {
                continue;
            };
            if search >= 0 && wave_spawned_at(&group, search) <= 0 {
                continue;
            }
            if !filter_type.is_empty() && dict_text(&group, "type") != filter_type {
                continue;
            }
            any = true;
            let card = build_wave_card(&node, index as i64, &group, expanded);
            let _ = list.bind_mut().add(&card).pad(2.0);
            list.bind_mut().row();
        }
        if groups.is_empty() {
            let label = factory::label(&kit::t("@waves.none").to_string());
            let _ = list.bind_mut().add(&label).pad(4.0);
            list.bind_mut().row();
        } else if !any {
            let label = factory::label(&kit::t("@none.found").to_string());
            let _ = list.bind_mut().add(&label).pad(4.0);
            list.bind_mut().row();
        }
        wave_refresh_graph(&node);
    }
}

/// `_build_edit_panel()`: copy / load / clear / reset.
fn wave_build_edit_panel(node: &Gd<MindDialog>) -> Gd<VBoxContainer> {
    // code-instantiated: the edit menu is a fixed four-action strip.
    let mut panel = VBoxContainer::new_alloc();
    panel.set_visible(false);
    let mut copy = factory::button(&kit::t("@copy.clipboard").to_string());
    let callable = Callable::from_fn("wave_copy", move |_args: &[&Variant]| {
        let json = editor_json("waves_json", &[]).unwrap_or_default();
        DisplayServer::singleton().clipboard_set(&json);
        ui_show_info(&kit::t("@waves.copied"));
    });
    copy.connect("pressed", &callable);
    panel.add_child(&copy);
    let mut load = factory::button(&kit::t("@load.clipboard").to_string());
    let target = node.clone();
    let callable = Callable::from_fn("wave_load", move |_args: &[&Variant]| {
        let text = DisplayServer::singleton().clipboard_get();
        if !as_bool(editor_call("set_waves_json", &[text.to_variant()])) {
            ui_show_info(&kit::t("@waves.invalid"));
            return;
        }
        let groups = wave_read_groups();
        let mut meta = target.clone();
        set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
        set_meta_bool(&mut meta, WAVE_EDIT_OPEN_META, false);
        request_refresh(&target, WAVE_REQUEST_BUILD);
    });
    load.connect("pressed", &callable);
    panel.add_child(&load);
    let mut clear = factory::button(&kit::t("@clear").to_string());
    let target = node.clone();
    let callable = Callable::from_fn("wave_clear", move |_args: &[&Variant]| {
        let mut meta = target.clone();
        set_meta_array(&mut meta, WAVE_GROUPS_META, &VarArray::new());
        wave_commit(&target);
        set_meta_bool(&mut meta, WAVE_EDIT_OPEN_META, false);
        request_refresh(&target, WAVE_REQUEST_BUILD);
    });
    clear.connect("pressed", &callable);
    panel.add_child(&clear);
    let mut reset = factory::button(&kit::t("@settings.reset").to_string());
    let target = node.clone();
    let callable = Callable::from_fn("wave_reset", move |_args: &[&Variant]| {
        let groups = editor_json("default_waves_json", &[])
            .map(|text| json_array(&text))
            .unwrap_or_default();
        let mut meta = target.clone();
        set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
        wave_commit(&target);
        set_meta_bool(&mut meta, WAVE_EDIT_OPEN_META, false);
        request_refresh(&target, WAVE_REQUEST_BUILD);
    });
    reset.connect("pressed", &callable);
    panel.add_child(&reset);
    panel
}

/// `_build_sort_panel()`: the sort keys + `@waves.sort.reverse`.
fn wave_build_sort_panel(node: &Gd<MindDialog>) -> Gd<VBoxContainer> {
    // code-instantiated: the sort panel is a fixed three-toggle strip.
    let mut panel = VBoxContainer::new_alloc();
    panel.set_visible(false);
    let mut sort_row = HBoxContainer::new_alloc();
    let current = meta_int(node, WAVE_SORT_META, 0);
    for (sort_value, name) in [(0i64, "begin"), (1, "health"), (2, "type")] {
        let mut button = factory::button(&kit::t(&format!("@waves.sort.{name}")).to_string());
        button.set_theme_type_variation("flatTogglet");
        button.set_toggle_mode(true);
        button.set_pressed(current == sort_value);
        let target = node.clone();
        let callable = Callable::from_fn("wave_sort_key", move |_args: &[&Variant]| {
            let mut meta = target.clone();
            set_meta_int(&mut meta, WAVE_SORT_META, sort_value);
            set_meta_bool(&mut meta, WAVE_SORT_OPEN_META, false);
            request_refresh(&target, WAVE_REQUEST_BUILD);
        });
        button.connect("pressed", &callable);
        sort_row.add_child(&button);
    }
    panel.add_child(&sort_row);
    let target = node.clone();
    let reverse = meta_bool(node, WAVE_REVERSE_META);
    let listener = Callable::from_fn("wave_sort_reverse", move |args: &[&Variant]| {
        let pressed = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        set_meta_bool(&mut target.clone(), WAVE_REVERSE_META, pressed);
        request_refresh(&target, WAVE_REQUEST_LIST);
    });
    panel.add_child(&mind_check(
        &kit::t("@waves.sort.reverse").to_string(),
        reverse,
        listener,
    ));
    panel
}

/// `SpawnGroup.getSpawned(wave)`: whether the group spawns on `wave`.
fn wave_spawned_at(group: &VarDictionary, wave: i64) -> i64 {
    let begin = dict_int(group, "begin");
    let end = match group.get("end") {
        Some(value) => as_int(Some(value)),
        None => WAVE_NEVER,
    };
    let spacing = dict_int(group, "spacing").max(1);
    if wave < begin || wave > end {
        return 0;
    }
    if (wave - begin) % spacing != 0 {
        return 0;
    }
    dict_int(group, "amount").max(1)
}

/// `groups.sort(Structs.comps(comparingFloat(sort.sort), comparingFloat(sort.secondary)))`.
fn wave_sort_groups(node: &Gd<MindDialog>) {
    let mut groups = meta_array(node, WAVE_GROUPS_META);
    if groups.is_empty() {
        return;
    }
    let sort = meta_int(node, WAVE_SORT_META, 0);
    let reverse = meta_bool(node, WAVE_REVERSE_META);
    let type_order = wave_unit_names();
    let health = wave_unit_health();
    let mut entries: Vec<Variant> = groups.iter_shared().collect();
    entries.sort_by(|first, second| {
        let first = first.try_to::<VarDictionary>().unwrap_or_default();
        let second = second.try_to::<VarDictionary>().unwrap_or_default();
        let key_a = wave_sort_key(&first, sort, &type_order, &health);
        let key_b = wave_sort_key(&second, sort, &type_order, &health);
        if key_a == key_b {
            return if sort == 0 {
                let a = wave_sort_key(&first, 2, &type_order, &health);
                let b = wave_sort_key(&second, 2, &type_order, &health);
                a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                dict_int(&first, "begin").cmp(&dict_int(&second, "begin"))
            };
        }
        key_a
            .partial_cmp(&key_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if reverse {
        entries.reverse();
    }
    for (index, value) in entries.iter().enumerate() {
        groups.set(index, value);
    }
    set_meta_array(&mut node.clone(), WAVE_GROUPS_META, &groups);
}

/// `_sort_key(group, sort)`.
fn wave_sort_key(
    group: &VarDictionary,
    sort: i64,
    type_order: &[String],
    health: &VarDictionary,
) -> f64 {
    match sort {
        1 => as_float(health.get(dict_text(group, "type").as_str())),
        2 => {
            let name = dict_text(group, "type");
            type_order
                .iter()
                .position(|entry| entry == &name)
                .map(|position| position as f64)
                .unwrap_or(-1.0)
        }
        _ => dict_int(group, "begin") as f64,
    }
}

/// One group card (`table.table(Tex.button, t -> ...)`).
fn build_wave_card(
    node: &Gd<MindDialog>,
    index: i64,
    group: &VarDictionary,
    expanded: i64,
) -> Gd<Control> {
    // code-instantiated: one card per spawn group; the count is data-driven.
    let mut card = PanelContainer::new_alloc();
    card.add_theme_stylebox_override("panel", &button_nine_patch("button"));
    let mut box_ = VBoxContainer::new_alloc();
    card.add_child(&box_);
    let mut header = Button::new_alloc();
    header.set_theme_type_variation("nonet");
    header.set_custom_minimum_size(Vector2::new(0.0, 34.0));
    let target = node.clone();
    let callable = Callable::from_fn("wave_expand", move |_args: &[&Variant]| {
        let mut meta = target.clone();
        let current = meta_int(&meta, WAVE_EXPANDED_META, -1);
        set_meta_int(
            &mut meta,
            WAVE_EXPANDED_META,
            if current == index { -1 } else { index },
        );
        request_refresh(&target, WAVE_REQUEST_LIST);
    });
    header.connect("pressed", &callable);
    box_.add_child(&header);
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_preset(LayoutPreset::FULL_RECT);
    row.set_mouse_filter(MouseFilter::IGNORE);
    row.add_theme_constant_override("separation", 0);
    header.add_child(&row);
    let unit_type = {
        let value = dict_text(group, "type");
        if value.is_empty() {
            String::from("?")
        } else {
            value
        }
    };
    // code-instantiated: the unit icon is resolved from the content atlas.
    let mut icon = TextureRect::new_alloc();
    if let Some(texture) = kit::icon_texture(&format!("unit-{unit_type}-ui")) {
        icon.set_texture(&texture);
    }
    icon.set_custom_minimum_size(Vector2::new(32.0, 32.0));
    icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
    icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    icon.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    icon.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&icon);
    let mut gap = Control::new_alloc();
    gap.set_custom_minimum_size(Vector2::new(3.0, 0.0));
    gap.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&gap);
    let mut name_label = factory::label(&wave_unit_name(&unit_type));
    name_label.add_theme_color_override("font_color", styles::ACCENT);
    let mut name_control = as_control(&name_label);
    name_control.set_custom_minimum_size(Vector2::new(110.0, 0.0));
    name_control.set_clip_contents(true);
    name_control.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    name_control.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&name_control);
    let mut spacer = Control::new_alloc();
    spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
    spacer.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&spacer);
    let begin = dict_int(group, "begin") + 1;
    let mut begin_label = factory::label(&begin.to_string());
    begin_label.add_theme_color_override("font_color", styles::ARC_LIGHT_GRAY);
    let mut begin_control = as_control(&begin_label);
    begin_control.set_custom_minimum_size(Vector2::new(45.0, 0.0));
    begin_control.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    begin_control.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&begin_control);
    let copy_target = node.clone();
    let copy = Callable::from_fn("wave_copy_group", move |_args: &[&Variant]| {
        wave_copy_group(&copy_target, index);
    });
    row.add_child(&wave_small_icon("copy", "@editor.copy", copy));
    let effect = dict_text(group, "effect");
    let effect_icon = if effect.is_empty() { "logic" } else { "warn" };
    let effect_tip = if effect.is_empty() {
        kit::t("@none").to_string()
    } else {
        effect.clone()
    };
    let effect_target = node.clone();
    let effect_action = Callable::from_fn("wave_effect", move |_args: &[&Variant]| {
        wave_show_effect_picker(&effect_target, index);
    });
    row.add_child(&wave_small_icon(effect_icon, &effect_tip, effect_action));
    let unit_target = node.clone();
    let unit_action = Callable::from_fn("wave_unit", move |_args: &[&Variant]| {
        wave_open_picker(
            &unit_target,
            "",
            &wave_unit_names(),
            "",
            &format!("unit:{index}"),
        );
    });
    row.add_child(&wave_small_icon("units", "@stat.unittype", unit_action));
    let remove_target = node.clone();
    let remove = Callable::from_fn("wave_remove", move |_args: &[&Variant]| {
        wave_remove_group(&remove_target, index);
    });
    row.add_child(&wave_small_icon("cancel", "@waves.remove", remove));
    if expanded == index {
        box_.add_child(&wave_group_fields(node, index, group));
    }
    card.upcast()
}

/// `group.type.localizedName` (`unit.<name>.name`).
fn wave_unit_name(unit_type: &str) -> String {
    let key = format!("@unit.{unit_type}.name");
    let value = kit::t(&key).to_string();
    if value == key || value.is_empty() {
        unit_type.to_string()
    } else {
        value
    }
}

/// `_small_icon(icon, tooltip, handler)`.
fn wave_small_icon(icon: &str, tooltip: &str, handler: Callable) -> Gd<Button> {
    let mut button = factory::icon_button(
        icon,
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    button.set_custom_minimum_size(Vector2::new(34.0, 34.0));
    button.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    for child in button.get_children().iter_shared() {
        if let Ok(row) = child.clone().try_cast::<HBoxContainer>() {
            for inner in row.get_children().iter_shared() {
                if let Ok(mut label) = inner.try_cast::<Label>() {
                    label.add_theme_font_size_override("font_size", 20);
                }
            }
        }
    }
    button.set_tooltip_text(&GString::from(tooltip));
    button.connect("pressed", &handler);
    button
}

/// `_group_fields(group, index)`: every `SpawnGroup` row.
fn wave_group_fields(node: &Gd<MindDialog>, index: i64, group: &VarDictionary) -> Gd<Control> {
    let mut fields = VBoxContainer::new_alloc();
    // `@waves.to` range + begin (`t.field("" + (group.begin + 1))`).
    let mut range_row = HBoxContainer::new_alloc();
    let begin = dict_int(group, "begin") + 1;
    let begin_target = node.clone();
    range_row.add_child(&wave_int_field(&begin.to_string(), 100.0, {
        Callable::from_fn("wave_begin", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            wave_set_field(&begin_target, index, "begin", (value - 1).to_variant());
        })
    }));
    range_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.to").to_string(),
    )));
    let end = match group.get("end") {
        Some(value) => as_int(Some(value)),
        None => WAVE_NEVER,
    };
    let end_text = if end == WAVE_NEVER {
        String::new()
    } else {
        (end + 1).to_string()
    };
    let end_target = node.clone();
    range_row.add_child(&wave_int_field(&end_text, 100.0, {
        Callable::from_fn("wave_end", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            wave_set_field(
                &end_target,
                index,
                "end",
                (if value > 0 { value - 1 } else { WAVE_NEVER }).to_variant(),
            );
        })
    }));
    fields.add_child(&range_row);
    // `@waves.every ... @waves.waves`.
    let mut spacing_row = HBoxContainer::new_alloc();
    spacing_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.every").to_string(),
    )));
    let spacing = match group.get("spacing") {
        Some(value) => as_int(Some(value)),
        None => 1,
    };
    let spacing_target = node.clone();
    spacing_row.add_child(&wave_int_field(&spacing.to_string(), 100.0, {
        Callable::from_fn("wave_spacing", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            if value > 0 {
                wave_set_field(&spacing_target, index, "spacing", value.to_variant());
            }
        })
    }));
    spacing_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.waves").to_string(),
    )));
    fields.add_child(&spacing_row);
    // amount + scaling (`1f / unitScaling` shown, Java's fixed(2)).
    let mut amount_row = HBoxContainer::new_alloc();
    let amount = match group.get("amount") {
        Some(value) => as_int(Some(value)),
        None => 1,
    };
    let amount_target = node.clone();
    amount_row.add_child(&wave_int_field(&amount.to_string(), 80.0, {
        Callable::from_fn("wave_amount", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            wave_set_field(&amount_target, index, "amount", value.to_variant());
        })
    }));
    let scaling = match group.get("scaling") {
        Some(value) => as_float(Some(value)),
        None => WAVE_NEVER as f64,
    };
    let per_spawn = if scaling <= 0.0 { 0.0 } else { 1.0 / scaling };
    amount_row.add_child(&as_control(&factory::label(" + ")));
    let scaling_target = node.clone();
    amount_row.add_child(&wave_float_field(&format!("{per_spawn:.2}"), 80.0, {
        Callable::from_fn("wave_scaling", move |args: &[&Variant]| {
            let value = as_float(args.first().map(|value| (*value).clone()));
            if value > 0.0 {
                wave_set_field(
                    &scaling_target,
                    index,
                    "scaling",
                    (1.0 / value).to_variant(),
                );
            }
        })
    }));
    amount_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.perspawn").to_string(),
    )));
    fields.add_child(&amount_row);
    // `@waves.max`.
    let mut max_row = HBoxContainer::new_alloc();
    let max = match group.get("max") {
        Some(value) => as_int(Some(value)),
        None => 40,
    };
    let max_target = node.clone();
    max_row.add_child(&wave_int_field(&max.to_string(), 80.0, {
        Callable::from_fn("wave_max", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            wave_set_field(&max_target, index, "max", value.to_variant());
        })
    }));
    max_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.max").to_string(),
    )));
    fields.add_child(&max_row);
    // `@waves.shields` + shieldScaling.
    let mut shield_row = HBoxContainer::new_alloc();
    let shields = match group.get("shields") {
        Some(value) => as_int(Some(value)),
        None => 0,
    };
    let shields_target = node.clone();
    shield_row.add_child(&wave_int_field(&shields.to_string(), 80.0, {
        Callable::from_fn("wave_shields", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            wave_set_field(&shields_target, index, "shields", value.to_variant());
        })
    }));
    shield_row.add_child(&as_control(&factory::label(" + ")));
    let shield_scaling = match group.get("shieldScaling") {
        Some(value) => as_int(Some(value)),
        None => 0,
    };
    let shield_scaling_target = node.clone();
    shield_row.add_child(&wave_int_field(&shield_scaling.to_string(), 80.0, {
        Callable::from_fn("wave_shield_scaling", move |args: &[&Variant]| {
            let value = as_int(args.first().map(|value| (*value).clone()));
            wave_set_field(
                &shield_scaling_target,
                index,
                "shieldScaling",
                value.to_variant(),
            );
        })
    }));
    shield_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.shields").to_string(),
    )));
    fields.add_child(&shield_row);
    // `@waves.guardian` (`group.effect == StatusEffects.boss`).
    let boss = dict_text(group, "effect") == "boss";
    let guardian_target = node.clone();
    let guardian = Callable::from_fn("wave_guardian", move |args: &[&Variant]| {
        let pressed = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        if pressed {
            wave_set_field(
                &guardian_target,
                index,
                "effect",
                GString::from("boss").to_variant(),
            );
        } else {
            wave_erase_field(&guardian_target, index, "effect");
        }
        request_refresh(&guardian_target, WAVE_REQUEST_LIST);
    });
    fields.add_child(&mind_check(
        &kit::t("@waves.guardian").to_string(),
        boss,
        guardian,
    ));
    // `@waves.team` row (`MapObjectivesDialog.showTeamSelect`).
    let mut team_row = HBoxContainer::new_alloc();
    team_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.team").to_string(),
    )));
    let mut team_button = factory::button(&wave_team_label(group));
    let team_target = node.clone();
    let callable = Callable::from_fn("wave_team", move |_args: &[&Variant]| {
        wave_open_picker(
            &team_target,
            "",
            &palette_team_names(),
            "@settings.resetKey",
            &format!("team:{index}"),
        );
    });
    team_button.connect("pressed", &callable);
    team_row.add_child(&team_button);
    fields.add_child(&team_row);
    // `@waves.spawn` row (packed `Point2` position).
    let mut spawn_row = HBoxContainer::new_alloc();
    spawn_row.add_child(&as_control(&factory::label(
        &kit::t("@waves.spawn").to_string(),
    )));
    let mut spawn_button = factory::button(&wave_spawn_label(group));
    let spawn_target = node.clone();
    let callable = Callable::from_fn("wave_spawn", move |_args: &[&Variant]| {
        wave_show_spawn_picker(&spawn_target, index);
    });
    spawn_button.connect("pressed", &callable);
    spawn_row.add_child(&spawn_button);
    fields.add_child(&spawn_row);
    fields.upcast()
}

/// `_int_field(value, width, setter)`: a field calling `setter(int(text))`.
fn wave_int_field(value: &str, width: f32, setter: Callable) -> Gd<LineEdit> {
    let mut field = text_field(value, 10);
    field.set_custom_minimum_size(Vector2::new(width, 0.0));
    let callable = Callable::from_fn("wave_int_field", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| value.to::<GString>().to_string())
            .unwrap_or_default();
        if let Some(number) = valid_int(&text) {
            let _ = setter.call(&[number.to_variant()]);
        }
    });
    field.connect("text_changed", &callable);
    field
}

/// `_float_field(value, width, setter)`.
fn wave_float_field(value: &str, width: f32, setter: Callable) -> Gd<LineEdit> {
    let mut field = text_field(value, 10);
    field.set_custom_minimum_size(Vector2::new(width, 0.0));
    let callable = Callable::from_fn("wave_float_field", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| value.to::<GString>().to_string())
            .unwrap_or_default();
        if let Some(number) = valid_float(&text) {
            let _ = setter.call(&[number.to_variant()]);
        }
    });
    field.connect("text_changed", &callable);
    field
}

/// `_commit()`: writes the working list back and refreshes the graph.
fn wave_commit(node: &Gd<MindDialog>) {
    let groups = meta_array(node, WAVE_GROUPS_META);
    let _ = editor_write("set_waves_json", &json_string(&groups.to_variant()));
    wave_refresh_graph(node);
}

/// Writes one field of one group, then commits.
fn wave_set_field(node: &Gd<MindDialog>, index: i64, key: &str, value: Variant) {
    let mut groups = meta_array(node, WAVE_GROUPS_META);
    if index < 0 || index >= groups.len() as i64 {
        return;
    }
    let Ok(mut group) = groups.at(index as usize).try_to::<VarDictionary>() else {
        return;
    };
    group.set(key, &value);
    groups.set(index as usize, &group.to_variant());
    set_meta_array(&mut node.clone(), WAVE_GROUPS_META, &groups);
    wave_commit(node);
}

/// Erases one group field (`group.erase("effect")`).
fn wave_erase_field(node: &Gd<MindDialog>, index: i64, key: &str) {
    let mut groups = meta_array(node, WAVE_GROUPS_META);
    if index < 0 || index >= groups.len() as i64 {
        return;
    }
    let Ok(mut group) = groups.at(index as usize).try_to::<VarDictionary>() else {
        return;
    };
    group.remove(key);
    groups.set(index as usize, &group.to_variant());
    set_meta_array(&mut node.clone(), WAVE_GROUPS_META, &groups);
    wave_commit(node);
}

/// `groups.insert(groups.indexOf(group) + 1, group.copy())`.
fn wave_copy_group(node: &Gd<MindDialog>, index: i64) {
    let mut groups = meta_array(node, WAVE_GROUPS_META);
    if index < 0 || index >= groups.len() as i64 {
        return;
    }
    let copy = groups
        .at(index as usize)
        .try_to::<VarDictionary>()
        .unwrap_or_default();
    groups.insert(index as usize + 1, &copy.duplicate_deep().to_variant());
    let mut meta = node.clone();
    set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
    set_meta_int(&mut meta, WAVE_EXPANDED_META, index + 1);
    wave_commit(node);
    request_refresh(node, WAVE_REQUEST_LIST);
}

fn wave_remove_group(node: &Gd<MindDialog>, index: i64) {
    let mut groups = meta_array(node, WAVE_GROUPS_META);
    if index < 0 || index >= groups.len() as i64 {
        return;
    }
    groups.remove(index as usize);
    let mut meta = node.clone();
    set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
    set_meta_int(&mut meta, WAVE_EXPANDED_META, -1);
    wave_commit(node);
    request_refresh(node, WAVE_REQUEST_LIST);
}

/// `showEffects(group)`: the status-effect picker.
fn wave_show_effect_picker(node: &Gd<MindDialog>, index: i64) {
    wave_open_picker(
        node,
        "",
        &wave_effect_names(),
        "@settings.resetKey",
        &format!("effect:{index}"),
    );
}

/// The spawn picker (`spawner.getSpawns()`: first 20 packed positions + all).
fn wave_show_spawn_picker(node: &Gd<MindDialog>, index: i64) {
    let mut names: Vec<String> = Vec::new();
    for value in editor_array("spawn_points_json").iter_shared() {
        if names.len() >= WAVE_SPAWN_CAP {
            break;
        }
        let packed = as_int(Some(value)) as i32;
        let (x, y) = unpack_point2(packed);
        names.push(format!("{x}, {y}"));
    }
    wave_open_picker(
        node,
        "@waves.spawn.select",
        &names,
        "@waves.spawn.all",
        &format!("spawn:{index}"),
    );
}

/// `_open_picker(title, items, reset_key, target)`.
fn wave_open_picker(
    node: &Gd<MindDialog>,
    title_key: &str,
    items: &[String],
    reset_key: &str,
    target: &str,
) {
    if kit::ui().is_none() {
        return;
    }
    set_meta_text(&mut node.clone(), WAVE_PICKER_META, target);
    wave_bind_picker(node);
    let mut list = VarArray::new();
    for item in items {
        list.push(&GString::from(item.as_str()).to_variant());
    }
    let mut context = VarDictionary::new();
    context.set("title", &GString::from(title_key).to_variant());
    context.set("items", &list.to_variant());
    context.set("reset", &GString::from(reset_key).to_variant());
    ui_open_dialog("list_select", &context);
}

/// `_connect_picker()`: the sibling `list_select` `selected` signal.
fn wave_bind_picker(node: &Gd<MindDialog>) {
    if node.has_meta(WAVE_PICKER_BOUND_META) {
        return;
    }
    let Some(mut picker) = dialog_node("list_select") else {
        return;
    };
    if !picker.has_signal("selected") {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("wave_picked", move |args: &[&Variant]| {
        let name = args
            .first()
            .map(|value| value.to::<GString>())
            .unwrap_or_default();
        set_meta_text(&mut target.clone(), WAVE_PICKED_META, &name.to_string());
        request_refresh_deferred(&target, WAVE_REQUEST_PICKED);
    });
    picker.connect("selected", &callable);
    node.clone()
        .set_meta(WAVE_PICKER_BOUND_META, &true.to_variant());
}

/// `_on_picked(picked_name)`: the pending picker target.
fn wave_apply_picked(node: &Gd<MindDialog>) {
    let target = meta_text(node, WAVE_PICKER_META);
    let name = meta_text(node, WAVE_PICKED_META);
    let mut meta = node.clone();
    set_meta_text(&mut meta, WAVE_PICKER_META, "");
    set_meta_text(&mut meta, WAVE_PICKED_META, "");
    let mut parts = target.splitn(2, ':');
    let kind = parts.next().unwrap_or_default();
    let index = parts.next().and_then(valid_int).unwrap_or(-1);
    match kind {
        "filter" => {
            set_meta_text(&mut node.clone(), WAVE_FILTER_META, &name);
        }
        "add" => {
            if !name.is_empty() {
                let mut groups = meta_array(node, WAVE_GROUPS_META);
                let mut group = VarDictionary::new();
                group.set("type", &GString::from(name.as_str()).to_variant());
                groups.push(&group);
                let mut meta = node.clone();
                set_meta_array(&mut meta, WAVE_GROUPS_META, &groups);
                set_meta_int(&mut meta, WAVE_EXPANDED_META, groups.len() as i64 - 1);
                wave_commit(node);
            }
        }
        "unit" => {
            if index >= 0 && !name.is_empty() {
                wave_set_field(
                    node,
                    index,
                    "type",
                    GString::from(name.as_str()).to_variant(),
                );
            }
        }
        "effect" => {
            if index >= 0 {
                if name.is_empty() {
                    wave_erase_field(node, index, "effect");
                } else {
                    wave_set_field(
                        node,
                        index,
                        "effect",
                        GString::from(name.as_str()).to_variant(),
                    );
                }
            }
        }
        "spawn" => {
            if index >= 0 {
                if name.is_empty() {
                    wave_set_field(node, index, "spawn", (-1i64).to_variant());
                } else {
                    let coords: Vec<&str> = name.split(',').collect();
                    if coords.len() == 2 {
                        let x = valid_int(coords[0].trim()).unwrap_or(0) as i32;
                        let y = valid_int(coords[1].trim()).unwrap_or(0) as i32;
                        wave_set_field(
                            node,
                            index,
                            "spawn",
                            i64::from(pack_point2(x, y)).to_variant(),
                        );
                    }
                }
            }
        }
        "team" if index >= 0 => {
            if name.is_empty() {
                wave_erase_field(node, index, "team");
            } else {
                let id = palette_team_names()
                    .iter()
                    .position(|team| team == &name)
                    .map(|position| position as i64)
                    .unwrap_or(-1);
                wave_set_field(node, index, "team", id.to_variant());
            }
        }
        _ => {}
    }
}

/// `_team_label(group)`.
fn wave_team_label(group: &VarDictionary) -> String {
    if group.get("team").is_none() {
        return kit::t("@none").to_string();
    }
    let id = dict_int(group, "team");
    palette_team_names()
        .get(id.max(0) as usize)
        .cloned()
        .unwrap_or_else(|| id.to_string())
}

/// `_spawn_label(group)`.
fn wave_spawn_label(group: &VarDictionary) -> String {
    let packed = match group.get("spawn") {
        Some(value) => as_int(Some(value)),
        None => -1,
    };
    if packed == -1 {
        return kit::t("@waves.spawn.all").to_string();
    }
    let (x, y) = unpack_point2(packed as i32);
    format!("{x}, {y}")
}

/// `_unit_names()` (`wave_units_json` rows).
fn wave_unit_rows() -> Vec<VarDictionary> {
    dict_rows(&editor_array("wave_units_json"))
}

fn wave_unit_names() -> Vec<String> {
    wave_unit_rows()
        .iter()
        .map(|row| dict_text(row, "name"))
        .collect()
}

/// `_unit_health()`.
fn wave_unit_health() -> VarDictionary {
    let mut health = VarDictionary::new();
    for row in wave_unit_rows() {
        health.set(
            &GString::from(dict_text(&row, "name").as_str()),
            &dict_float(&row, "health", 0.0).to_variant(),
        );
    }
    health
}

/// `_effect_names()` (`wave_effects_json`).
fn wave_effect_names() -> Vec<String> {
    editor_array("wave_effects_json")
        .iter_shared()
        .map(|value| variant_text(&value))
        .collect()
}

/// `_read_groups()`: the `spawns` tag, else the vanilla table.
fn wave_read_groups() -> VarArray {
    let parsed = editor_json("waves_json", &[])
        .map(|text| json_array(&text))
        .unwrap_or_default();
    if !parsed.is_empty() {
        return parsed;
    }
    // `state.rules.spawns.isEmpty() ? waves.get()`.
    editor_json("default_waves_json", &[])
        .map(|text| json_array(&text))
        .unwrap_or_default()
}

/// `graph.groups = groups; graph.rebuild()`.
fn wave_refresh_graph(node: &Gd<MindDialog>) {
    if let Some(mut graph) = find_named(node, WAVE_GRAPH_NODE)
        && graph.has_method("refresh")
    {
        let _ = graph.call("refresh", &[]);
    }
}

/// Instantiates `res://scenes/editor/wave_graph.tscn` (`graphScene`).
fn instantiate_wave_graph() -> Option<Gd<Control>> {
    let scene = godot::tools::try_load::<godot::classes::PackedScene>(&GString::from(
        "res://scenes/editor/wave_graph.tscn",
    ))
    .ok()?;
    let node = scene.instantiate()?;
    let mut control = node.try_cast::<Control>().ok()?;
    control.set_name(WAVE_GRAPH_NODE);
    control.set_h_size_flags(SizeFlags::EXPAND_FILL);
    control.set_v_size_flags(SizeFlags::EXPAND_FILL);
    control.set_custom_minimum_size(Vector2::new(240.0, 260.0));
    Some(control)
}

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "wave_info" => Some(Box::new(WaveInfoDialog::default())),
        _ => None,
    }
}
