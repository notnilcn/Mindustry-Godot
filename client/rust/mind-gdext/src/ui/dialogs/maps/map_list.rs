// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared map-list helpers for the maps dialogs. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/MapListDialog.java

use crate::gdutil::variant::{parse_object, to_text_godot};
use crate::ui::dialog::MindDialog;
use crate::ui::dialogs::campaign::util::*;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindTable, styles};
use godot::builtin::VarDictionary;
use godot::classes::control::{GrowDirection, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::text_server::OverrunBehavior;
use godot::classes::{
    Button, ColorRect, Control, HBoxContainer, Label, LineEdit, MarginContainer, PanelContainer,
    VBoxContainer,
};
use godot::prelude::*;
use mind_core::util::strip_colors;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// CustomGameDialog (MapListDialog base, display_type = false)
// ---------------------------------------------------------------------------

/// `MapListDialog.settings` persistence key (`Core.settings.getJson`).
const MAPLIST_SETTINGS_KEY: &str = "editormapviewsettings";
/// `rebuildMaps`: `float mapsize = 200f`.
pub(crate) const MAP_SIZE: f32 = 200.0;
/// `Core.graphics.getWidth() / Scl.scl(230)` columns.
pub(crate) const MAP_COLUMN_WIDTH: f32 = 230.0;
/// `button.margin(9)`.
pub(crate) const MAP_CARD_MARGIN: i32 = 9;
/// `Gamemode.all` in declaration order (`Gamemode.java:9-43`).
const GAMEMODES: [&str; 5] = ["survival", "sandbox", "attack", "pvp", "editor"];
/// Gamemodes whose `mode<Name>` icon exists in the atlas.
const MODE_ICONS: [(&str, &str); 3] = [
    ("survival", "mode-survival"),
    ("attack", "mode-attack"),
    ("pvp", "mode-pvp"),
];
/// `MapPriority.all` in declaration order with the filter-dialog icons.
const PRIORITIES: [&str; 4] = ["name", "custom", "builtin", "recent"];
const PRIORITY_ICONS: [&str; 4] = ["file-text", "players", "hammer", "play"];

/// `MapViewSettings` defaults (`MapListDialog.java:58-75`).
pub(crate) fn maplist_default_settings() -> VarDictionary {
    let mut settings = VarDictionary::new();
    settings.set("showBuiltIn", &true.to_variant());
    settings.set("showCustom", &true.to_variant());
    settings.set("showModded", &true.to_variant());
    settings.set("searchAuthor", &false.to_variant());
    settings.set("searchDescription", &false.to_variant());
    settings.set("searchModname", &false.to_variant());
    settings.set("priority", &gs("recent").to_variant());
    settings
}

/// `showMap(map)` override hook (`MapListDialog._show_map` is virtual).
pub(crate) type MapShowHook = Rc<dyn Fn(&mut MindDialog, &VarDictionary)>;

/// Controller state for the reused map-browser dialog node (`CustomGameDialog`
/// and `EditorMapsDialog` share this node type).
#[derive(Default)]
pub(crate) struct MapListState {
    pub(crate) settings: VarDictionary,
    /// `MapListDialog.modes`: the gamemode filter (not persisted).
    pub(crate) mode_filter: Vec<String>,
    /// `searchString` (lowercased; empty = null).
    pub(crate) search: String,
    pub(crate) field: Option<Gd<LineEdit>>,
    pub(crate) grid: Option<Gd<MindTable>>,
    pub(crate) filter_panel: Option<Gd<Control>>,
    pub(crate) priority_buttons: Vec<Gd<Button>>,
    /// `MapViewSettings` flag -> its filter toggle (re-synced on reload).
    pub(crate) setting_buttons: Vec<(String, Gd<Button>)>,
    /// Per-map `Gamemode.valid` cache (`MindPreview.map_modes`), keyed by path.
    pub(crate) modes_cache: Vec<(String, VarDictionary)>,
    /// Coalesces `previews_changed` batches into one grid rebuild per frame.
    pub(crate) preview_refresh_queued: bool,
    /// `MapListDialog.displayType`: `EditorMapsDialog` turns it on to show the
    /// custom/built-in/workshop source label under each card.
    pub(crate) display_type: bool,
    /// `showMap(map)` override: `EditorMapsDialog` opens the per-map `editor_map`
    /// page, `CustomGameDialog` (the default, `None`) emits `map_chosen` and
    /// opens `map_play` (`MapListDialog._show_map` virtual).
    pub(crate) show_map: Option<MapShowHook>,
    pub(crate) deferred: Deferred,
}

impl MapListState {
    pub(crate) fn new() -> Self {
        Self {
            settings: maplist_default_settings(),
            ..Self::default()
        }
    }
}
pub(crate) fn maplist_build(dialog: &mut MindDialog, state: &Rc<RefCell<MapListState>>) {
    maplist_build_search(dialog, state);
    // code-instantiated: tile count and layout are driven by the Maps registry.
    let mut grid = MindTable::new_alloc();
    grid.set_mouse_filter(MouseFilter::PASS);
    let mut root = dialog.content_table();
    // `cont.add(pane).padLeft(28f)` + `mapTable.marginRight(12f)`; the table is
    // centred in the pane (`Table` default alignment), so expand (not fill).
    root.bind_mut()
        .add(&grid)
        .expand_x()
        .pad_left(28.0)
        .pad_right(12.0);
    state.borrow_mut().grid = Some(grid);
    dialog.clear_buttons();
    // `setup`: `buttons.button("@back", Icon.left, this::hide).size(210f, 64f)`.
    let close = dialog.base().callable("close_pressed");
    maplist_add_pane_button(dialog, "@back", "left", Some(close));
    maplist_rebuild(dialog, state);
}

/// `buttons.button(text, icon, listener).size(210f, 64f)` over the default
/// `Tex.button` pane (`BaseDialog.addCloseButton` look).
pub(crate) fn maplist_add_pane_button(
    dialog: &mut MindDialog,
    text_key: &str,
    icon_name: &str,
    listener: Option<Callable>,
) -> Gd<Button> {
    // code-instantiated: the dialog's button row is data-driven per dialog.
    let mut button = factory::button("");
    button.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    dialog.apply_pane_button(&mut button, OUTLINE_GRAY, 4.0);
    let glyph: Gd<Control> = glyph_control(icon_name, 24.0, Color::WHITE).upcast();
    dialog.build_button_content(&mut button, Some(glyph), &kit::t(text_key).to_string());
    if let Some(listener) = listener
        && listener.is_valid()
    {
        button.connect("pressed", &listener);
    }
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&button);
    }
    button
}

/// `search.image(Icon.zoom)`, `search.field(...).maxTextLength(50).growX()`,
/// `search.button(Icon.filter, Styles.emptyi, this::showMapFilters)`.
pub(crate) fn maplist_build_search(dialog: &mut MindDialog, state: &Rc<RefCell<MapListState>>) {
    // code-instantiated: the search row is a fixed composite (zoom glyph + field
    // + filter) over the runtime map grid.
    let mut search = HBoxContainer::new_alloc();
    search.set_name("Search");
    search.add_theme_constant_override("separation", 6);
    let zoom = glyph_control("zoom", 24.0, Color::WHITE);
    search.add_child(&zoom);
    let mut field = factory::field(&kit::t("@editor.search").to_string());
    field.set_max_length(50);
    field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let target = dialog.to_gd();
        let state_for_search = state.clone();
        let callable = Callable::from_fn("maplist_search", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            state_for_search.borrow_mut().search = text.to_lowercase();
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            maplist_rebuild(&mut bound, &state_for_search);
        });
        field.connect("text_changed", &callable);
    }
    search.add_child(&field);
    let mut filter_button = factory::icon_button(
        "filter",
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    filter_button.set_tooltip_text(&kit::t("@editor.filters"));
    filter_button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
    let target = dialog.to_gd();
    let state_for_filters = state.clone();
    let callable = Callable::from_fn("maplist_toggle_filters", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        maplist_toggle_filters(&mut bound, &state_for_filters);
    });
    filter_button.connect("pressed", &callable);
    search.add_child(&filter_button);
    let mut filter_panel = maplist_build_filter_panel(dialog, state);
    filter_panel.set_visible(false);
    let layout = dialog
        .base()
        .get_node_or_null("Center/Panel/Layout")
        .and_then(|node| node.try_cast::<VBoxContainer>().ok());
    match (layout, dialog.body()) {
        (Some(mut layout), Some(body)) => {
            // `cont.add(search)` takes `cont.defaults().pad(3)` above it.
            // code-instantiated: wraps the code-built search row.
            let mut search_pad = MarginContainer::new_alloc();
            search_pad.set_name("SearchPad");
            search_pad.add_theme_constant_override("margin_top", 3);
            search_pad.add_child(&search);
            layout.add_child(&search_pad);
            let search_node: Gd<Node> = search_pad.clone().upcast();
            layout.move_child(&search_node, body.get_index());
            layout.add_child(&filter_panel);
            let panel_node: Gd<Node> = filter_panel.clone().upcast();
            layout.move_child(&panel_node, body.get_index());
        }
        _ => {
            let mut root = dialog.content_table();
            root.bind_mut().add(&search).grow_x();
            root.bind_mut().row();
            root.bind_mut().add(&filter_panel).grow_x();
        }
    }
    let mut st = state.borrow_mut();
    st.field = Some(field);
    let control: Gd<Control> = filter_panel.clone().upcast();
    st.filter_panel = Some(control);
}

/// `showMapFilters`: gamemode + priority toggles, the map type toggles and the
/// search-in toggles (upstream opens them in a `BaseDialog`; the port keeps them
/// in an inline panel under the search row).
pub(crate) fn maplist_build_filter_panel(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
) -> Gd<PanelContainer> {
    // code-instantiated: the filter panel is a fixed upstream toggle set.
    let mut panel = PanelContainer::new_alloc();
    panel.set_name("Filters");
    panel.set_theme_type_variation("smallPane");
    let mut menu = VBoxContainer::new_alloc();
    menu.add_theme_constant_override("separation", 6);
    panel.add_child(&menu);
    let mut tabs = HBoxContainer::new_alloc();
    tabs.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    tabs.add_theme_constant_override("separation", 10);
    menu.add_child(&tabs);
    let tabs_node: Gd<Node> = tabs.clone().upcast();
    // `t.add("@editor.filters.mode")` + one `Styles.emptyTogglei` 60 px toggle
    // per gamemode with an icon.
    let mut mode_box = maplist_filter_box(&tabs_node, "@editor.filters.mode");
    for mode in GAMEMODES {
        let Some((_, icon)) = MODE_ICONS.iter().find(|(name, _)| *name == mode) else {
            continue;
        };
        let mut mode_button = factory::glyph_button(
            icon,
            &kit::t(&format!("@mode.{mode}.name")).to_string(),
            "emptyTogglei",
        );
        mode_button.set_toggle_mode(true);
        mode_button.set_custom_minimum_size(Vector2::new(60.0, 60.0));
        mode_button.set_pressed(state.borrow().mode_filter.iter().any(|value| value == mode));
        mode_button.set_tooltip_text(&kit::t(&format!("@mode.{mode}.name")));
        let target = dialog.to_gd();
        let state_for_mode = state.clone();
        let mode_name = mode.to_string();
        let callable = Callable::from_fn("maplist_mode_toggle", move |args: &[&Variant]| {
            let pressed = args
                .first()
                .map(|value| (*value).to::<bool>())
                .unwrap_or(false);
            {
                let mut st = state_for_mode.borrow_mut();
                let present = st.mode_filter.contains(&mode_name);
                if pressed && !present {
                    st.mode_filter.push(mode_name.clone());
                } else if !pressed && present {
                    st.mode_filter.retain(|value| *value != mode_name);
                }
            }
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            maplist_rebuild(&mut bound, &state_for_mode);
        });
        mode_button.connect("toggled", &callable);
        mode_box.add_child(&mode_button);
    }
    // `t.add("@editor.filters.priorities")`: one checked-when-current toggle per
    // `MapPriority`.
    let mut priority_box = maplist_filter_box(&tabs_node, "@editor.filters.priorities");
    state.borrow_mut().priority_buttons.clear();
    for (index, priority) in PRIORITIES.iter().enumerate() {
        let mut priority_button =
            factory::glyph_button(PRIORITY_ICONS[index], priority, "emptyTogglei");
        priority_button.set_toggle_mode(true);
        priority_button.set_custom_minimum_size(Vector2::new(60.0, 60.0));
        priority_button
            .set_tooltip_text(&kit::t(&format!("@editor.filters.prioritize.{priority}")));
        let target = dialog.to_gd();
        let state_for_priority = state.clone();
        let priority_name = priority.to_string();
        let callable = Callable::from_fn("maplist_priority", move |_args: &[&Variant]| {
            {
                let mut st = state_for_priority.borrow_mut();
                st.settings.set("priority", &gs(priority_name.as_str()));
            }
            maplist_sync_priority_buttons(&state_for_priority);
            maplist_save_settings(&state_for_priority);
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            maplist_rebuild(&mut bound, &state_for_priority);
        });
        priority_button.connect("pressed", &callable);
        priority_box.add_child(&priority_button);
        state.borrow_mut().priority_buttons.push(priority_button);
    }
    maplist_sync_priority_buttons(state);
    // `menu.add("@editor.filters.type")` + custom/builtin/modded (150x60).
    let menu_node: Gd<Node> = menu.clone().upcast();
    let mut type_box = maplist_filter_box(&menu_node, "@editor.filters.type");
    maplist_add_setting_toggle(dialog, state, &mut type_box, "@custom", "showCustom");
    maplist_add_setting_toggle(dialog, state, &mut type_box, "@builtin", "showBuiltIn");
    maplist_add_setting_toggle(dialog, state, &mut type_box, "@modded", "showModded");
    // `menu.add("@editor.filters.search")` + author/description/modname.
    let mut search_box = maplist_filter_box(&menu_node, "@editor.filters.search");
    maplist_add_setting_toggle(
        dialog,
        state,
        &mut search_box,
        "@editor.filters.author",
        "searchAuthor",
    );
    maplist_add_setting_toggle(
        dialog,
        state,
        &mut search_box,
        "@editor.filters.description",
        "searchDescription",
    );
    maplist_add_setting_toggle(
        dialog,
        state,
        &mut search_box,
        "@editor.filters.modname",
        "searchModname",
    );
    panel
}

/// A titled `Tex.button` box (`t.add(title).row(); t.table(Tex.button, ...)`);
/// returns the row the toggles go into.
pub(crate) fn maplist_filter_box(parent: &Gd<Node>, title_key: &str) -> Gd<HBoxContainer> {
    // code-instantiated: filter group boxes are part of the fixed filter panel.
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 6);
    let label = factory::label(&kit::t(title_key).to_string());
    column.add_child(&label);
    let mut box_ = PanelContainer::new_alloc();
    let pane = MindDialog::pane_box(Color::BLACK, OUTLINE_GRAY, 4.0, 2.0, -1.0);
    box_.add_theme_stylebox_override("panel", &pane);
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 0);
    box_.add_child(&row);
    column.add_child(&box_);
    let mut parent = parent.clone();
    parent.add_child(&column);
    row
}

/// A `Styles.flatTogglet` 150x60 toggle bound to one `MapViewSettings` flag.
pub(crate) fn maplist_add_setting_toggle(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    row: &mut Gd<HBoxContainer>,
    key: &str,
    setting: &str,
) {
    // code-instantiated: filter toggles are data-driven (one per settings flag).
    let mut button = factory::button(&kit::t(key).to_string());
    button.set_theme_type_variation("flatTogglet");
    button.set_toggle_mode(true);
    button.set_custom_minimum_size(Vector2::new(150.0, 60.0));
    button.set_pressed(row_bool(&state.borrow().settings, setting));
    let target = dialog.to_gd();
    let state_for_toggle = state.clone();
    let setting_name = setting.to_string();
    let callable = Callable::from_fn("maplist_setting_toggle", move |args: &[&Variant]| {
        let pressed = args
            .first()
            .map(|value| (*value).to::<bool>())
            .unwrap_or(false);
        {
            let mut st = state_for_toggle.borrow_mut();
            st.settings
                .set(&gs(setting_name.as_str()), &pressed.to_variant());
        }
        maplist_save_settings(&state_for_toggle);
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        maplist_rebuild(&mut bound, &state_for_toggle);
    });
    button.connect("toggled", &callable);
    row.add_child(&button);
    state
        .borrow_mut()
        .setting_buttons
        .push((setting.to_string(), button));
}

pub(crate) fn maplist_sync_priority_buttons(state: &Rc<RefCell<MapListState>>) {
    let (buttons, priority) = {
        let st = state.borrow();
        (
            st.priority_buttons.clone(),
            row_str(&st.settings, "priority"),
        )
    };
    for (index, button) in buttons.iter().enumerate() {
        let Some(priority_name) = PRIORITIES.get(index) else {
            continue;
        };
        set_toggle(button, *priority_name == priority);
    }
}

pub(crate) fn maplist_toggle_filters(dialog: &mut MindDialog, state: &Rc<RefCell<MapListState>>) {
    let _ = dialog;
    let panel = {
        let st = state.borrow();
        st.filter_panel.clone()
    };
    let Some(mut panel) = panel else {
        return;
    };
    let visible = panel.is_visible();
    panel.set_visible(!visible);
}

/// `Core.settings.getJson("editormapviewsettings", ...)`: shared by both map
/// dialogs, so it is re-read on every `setup`.
pub(crate) fn maplist_load_settings(dialog: &mut MindDialog, state: &Rc<RefCell<MapListState>>) {
    let _ = dialog;
    let text = setting_text(MAPLIST_SETTINGS_KEY, "");
    if text.trim().is_empty() {
        return;
    }
    let stored = parse_object(&text);
    let keys: Vec<String> = state
        .borrow()
        .settings
        .keys_array()
        .iter_shared()
        .map(|value| to_text_godot(&value))
        .collect();
    {
        let mut st = state.borrow_mut();
        for key in keys {
            if let Some(value) = stored.get(&gs(key.as_str())) {
                st.settings.set(&gs(key.as_str()), &value);
            }
        }
        let priority = row_str(&st.settings, "priority");
        if !PRIORITIES.contains(&priority.as_str()) {
            st.settings.set("priority", &gs("recent"));
        }
    }
    maplist_sync_priority_buttons(state);
    let buttons = state.borrow().setting_buttons.clone();
    for (setting, button) in buttons {
        let pressed = row_bool(&state.borrow().settings, &setting);
        set_toggle(&button, pressed);
    }
}

/// `MapViewSettings.save` (`activeDialog.hidden(settings::save)`).
pub(crate) fn maplist_save_settings(state: &Rc<RefCell<MapListState>>) {
    let settings = {
        let st = state.borrow();
        st.settings.clone()
    };
    setting_put_text(MAPLIST_SETTINGS_KEY, &json_stringify(&settings));
}

/// Reloads the live `MindPreview` registry and asks it for the missing previews
/// (`Maps.loadPreviews` + `createAllPreviews`); batches land through
/// `previews_changed`.
pub(crate) fn maplist_refresh_registry(dialog: &mut MindDialog, state: &Rc<RefCell<MapListState>>) {
    state.borrow_mut().modes_cache.clear();
    let Some(preview) = preview_node() else {
        return;
    };
    let mut node: Gd<Node> = preview.upcast();
    if node.has_method("refresh") {
        node.call("refresh", &[]);
    }
    let target = dialog.to_gd();
    let state_for_preview = state.clone();
    let callable = Callable::from_fn("maplist_previews_changed", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        maplist_on_previews_changed(&mut bound, &state_for_preview);
    });
    connect_once(
        dialog,
        "maplist_previews",
        &node,
        "previews_changed",
        &callable,
    );
    if node.has_method("generate_previews") {
        node.call("generate_previews", &[]);
    }
}

/// A generated preview batch landed (`Maps.createAllPreviews`); its scan facts
/// (`Map.spawns`/`teams`) now exist, so drop the mode cache and refresh the
/// tiles once per frame while batches keep arriving.
pub(crate) fn maplist_on_previews_changed(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
) {
    if state.borrow().preview_refresh_queued {
        return;
    }
    let deferred = {
        let st = state.borrow();
        st.deferred.clone()
    };
    let target = dialog.to_gd();
    let state_for_rebuild = state.clone();
    state.borrow_mut().preview_refresh_queued = true;
    deferred.push(move || {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        maplist_rebuild_after_previews(&mut bound, &state_for_rebuild);
    });
}

pub(crate) fn maplist_rebuild_after_previews(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
) {
    state.borrow_mut().preview_refresh_queued = false;
    state.borrow_mut().modes_cache.clear();
    maplist_rebuild(dialog, state);
}

/// The live registry rows (`MindPreview.maps_list`), else the `campaign_views`
/// `maps` fallback for an empty registry.
pub(crate) fn maplist_maps(dialog: &mut MindDialog) -> Vec<VarDictionary> {
    let mut rows: Vec<VarDictionary> = Vec::new();
    if let Some(preview) = preview_node() {
        for row in preview.bind().maps_list().iter_shared() {
            if !row_str(&row, "name").is_empty() {
                rows.push(row);
            }
        }
    }
    if rows.is_empty() {
        for row_variant in dialog.campaign_section("maps").iter_shared() {
            let Ok(row) = row_variant.try_to::<VarDictionary>() else {
                continue;
            };
            if !row_str(&row, "name").is_empty() {
                rows.push(row);
            }
        }
    }
    rows
}

/// One sortable map row (`MapListDialog._sorted_maps` keyed entry).
struct MapSortKey {
    row: VarDictionary,
    index: usize,
    name: String,
    custom: bool,
    modified: i64,
}

/// `maps.customMaps()` + `defaultMaps()` + `moddedMaps()` (each list filtered
/// by its `MapViewSettings` flag), `distinct()`, then
/// `sort(settings.priority.comparator)`.
pub(crate) fn maplist_sorted_maps(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
) -> Vec<VarDictionary> {
    let mut customs: Vec<VarDictionary> = Vec::new();
    let mut builtins: Vec<VarDictionary> = Vec::new();
    let mut modded: Vec<VarDictionary> = Vec::new();
    for row in maplist_maps(dialog) {
        if row_bool(&row, "custom") {
            customs.push(row);
        } else if row_bool(&row, "mod") {
            modded.push(row);
        } else {
            builtins.push(row);
        }
    }
    let settings = {
        let st = state.borrow();
        st.settings.clone()
    };
    let mut entries: Vec<VarDictionary> = Vec::new();
    if row_bool_or(&settings, "showCustom", true) {
        entries.extend(customs);
    }
    if row_bool_or(&settings, "showBuiltIn", true) {
        entries.extend(builtins);
    }
    if row_bool_or(&settings, "showModded", true) {
        entries.extend(modded);
    }
    // `Seq.sort` is stable (TimSort); carry the list index as the final key.
    let mut keyed: Vec<MapSortKey> = entries
        .iter()
        .enumerate()
        .map(|(index, row)| MapSortKey {
            row: row.clone(),
            index,
            name: row_str(row, "name"),
            custom: row_bool(row, "custom"),
            modified: row_i64(row, "lastModified"),
        })
        .collect();
    let priority = row_opt_str(&settings, "priority").unwrap_or_else(|| String::from("recent"));
    keyed.sort_by(|a, b| maplist_compare(&priority, a, b));
    keyed.into_iter().map(|entry| entry.row).collect()
}

/// `MapPriority` comparators (`MapListDialog.java:44-50`); a negative result
/// orders `a` first. `String.compareTo` is a code-point compare.
fn maplist_compare(priority: &str, a: &MapSortKey, b: &MapSortKey) -> Ordering {
    let mut result = match priority {
        "name" => a.name.cmp(&b.name),
        "custom" => {
            // `comparingBool(m -> !m.custom)` then `comparingLong(-lastModified)`.
            let first = (if a.custom { 0 } else { 1 }).cmp(&(if b.custom { 0 } else { 1 }));
            if first == Ordering::Equal {
                (-a.modified).cmp(&(-b.modified))
            } else {
                first
            }
        }
        "builtin" => {
            let first = (if a.custom { 1 } else { 0 }).cmp(&(if b.custom { 1 } else { 0 }));
            if first == Ordering::Equal {
                (-a.modified).cmp(&(-b.modified))
            } else {
                first
            }
        }
        _ => {
            // `recent`: `comparingLong(-lastModified)` then `comparing(Map::name)`.
            let first = (-a.modified).cmp(&(-b.modified));
            if first == Ordering::Equal {
                a.name.cmp(&b.name)
            } else {
                first
            }
        }
    };
    if result == Ordering::Equal {
        result = (a.index as i64).cmp(&(b.index as i64));
    }
    result
}

/// `Gamemode.valid(map)` (`Gamemode.java`): survival needs `spawns > 0`,
/// attack/pvp need `teams.size > 1`, sandbox/editor are always valid.
pub(crate) fn maplist_map_modes(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    map: &VarDictionary,
) -> VarDictionary {
    let _ = dialog;
    let key = row_opt_str(map, "path").unwrap_or_else(|| row_str(map, "name"));
    if let Some((_, cached)) = state
        .borrow()
        .modes_cache
        .iter()
        .find(|(cached_key, _)| *cached_key == key)
    {
        return cached.clone();
    }
    let mut modes = VarDictionary::new();
    if row_bool_or(map, "preview", true)
        && let Some(mut preview) = preview_node()
    {
        modes = preview.bind_mut().map_modes(gs(row_str(map, "name")));
    }
    state.borrow_mut().modes_cache.push((key, modes.clone()));
    modes
}

pub(crate) fn maplist_mode_valid(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    map: &VarDictionary,
    mode: &str,
) -> bool {
    if mode == "sandbox" || mode == "editor" {
        return true;
    }
    row_bool_or(&maplist_map_modes(dialog, state, map), mode, false)
}

/// The gamemode filter (`invalid |= !mode.valid(map)`) and the search over
/// `plainName` (+ author/description/mod name per `MapViewSettings`).
pub(crate) fn maplist_passes_filter(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    map: &VarDictionary,
) -> bool {
    let (mode_filter, search, settings) = {
        let st = state.borrow();
        (
            st.mode_filter.clone(),
            st.search.clone(),
            st.settings.clone(),
        )
    };
    for mode in &mode_filter {
        if !maplist_mode_valid(dialog, state, map, mode) {
            return false;
        }
    }
    if search.is_empty() {
        return true;
    }
    let needle = search.as_str();
    if strip_colors(&row_str(map, "name"))
        .to_lowercase()
        .contains(needle)
    {
        return true;
    }
    if row_bool(&settings, "searchAuthor")
        && strip_colors(&row_str(map, "author"))
            .to_lowercase()
            .contains(needle)
    {
        return true;
    }
    if row_bool(&settings, "searchDescription")
        && strip_colors(&row_str(map, "description"))
            .to_lowercase()
            .contains(needle)
    {
        return true;
    }
    if row_bool(&settings, "searchModname")
        && strip_colors(&row_str(map, "mod_name"))
            .to_lowercase()
            .contains(needle)
    {
        return true;
    }
    false
}

/// `MapListDialog.rebuildMaps`: one card per filtered map, `maxwidth` per row.
pub(crate) fn maplist_rebuild(dialog: &mut MindDialog, state: &Rc<RefCell<MapListState>>) {
    let Some(mut grid) = state.borrow().grid.clone() else {
        return;
    };
    grid.bind_mut().clear();
    let maxwidth = ((dialog.base().get_viewport_rect().size.x / MAP_COLUMN_WIDTH) as i64).max(1);
    let mut index = 0i64;
    let mut shown_any = false;
    for map in maplist_sorted_maps(dialog, state) {
        if !maplist_passes_filter(dialog, state, &map) {
            continue;
        }
        shown_any = true;
        if index > 0 && index % maxwidth == 0 {
            grid.bind_mut().row();
        }
        // `mapTable.button("", Styles.grayt, ...).width(mapsize).bottom().pad(8)`.
        let tile = maplist_build_map_tile(dialog, state, &map);
        grid.bind_mut().add(&tile).width(MAP_SIZE).bottom().pad(8.0);
        index += 1;
    }
    if !shown_any {
        let label = factory::label(&kit::tm("@maps.none").to_string());
        grid.bind_mut().add(&label).pad(8.0);
    }
}

/// One `rebuildMaps` card (`MapListDialog.java:165-205`).
pub(crate) fn maplist_build_map_tile(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    map: &VarDictionary,
) -> Gd<Button> {
    let map_name = row_str(map, "name");
    // code-instantiated: map tiles are data-driven from the Maps registry.
    let mut tile = factory::button("");
    tile.set_theme_type_variation("grayt");
    apply_grayt(&mut tile);
    tile.set_custom_minimum_size(Vector2::new(MAP_SIZE, 0.0));
    tile.set_clip_contents(true);
    {
        let target = dialog.to_gd();
        let state_for_pick = state.clone();
        let map_for_pick = map.clone();
        let callable = Callable::from_fn("maplist_choose_map", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            maplist_show_map_for(&mut bound, &state_for_pick, &map_for_pick);
        });
        tile.connect("pressed", &callable);
    }
    // code-instantiated: `button.margin(9)` inset around the card column.
    let mut inset = MarginContainer::new_alloc();
    inset.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    inset.set_mouse_filter(MouseFilter::IGNORE);
    for side in ["left", "right", "top", "bottom"] {
        inset.add_theme_constant_override(&format!("margin_{side}"), MAP_CARD_MARGIN);
    }
    tile.add_child(&inset);
    // code-instantiated: the card column (`button.bottom()`: content at the
    // bottom).
    let mut column = VBoxContainer::new_alloc();
    column.set_alignment(godot::classes::box_container::AlignmentMode::END);
    column.set_mouse_filter(MouseFilter::IGNORE);
    column.add_theme_constant_override("separation", 0);
    inset.add_child(&column);
    let mode_row = maplist_build_mode_row(dialog, state, map);
    column.add_child(&mode_row);
    // `button.add(map.name()).width(mapsize - 18f).center().get().setEllipsis(true)`.
    // code-instantiated: the name needs native ellipsis trimming the rich label
    // lacks.
    let mut name_label = Label::new_alloc();
    name_label.set_text(&gs(map_name.clone()));
    name_label.set_clip_text(true);
    name_label.set_text_overrun_behavior(OverrunBehavior::TRIM_ELLIPSIS);
    name_label.set_custom_minimum_size(Vector2::new(MAP_SIZE - 18.0, 0.0));
    name_label.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    name_label.set_mouse_filter(MouseFilter::IGNORE);
    column.add_child(&name_label);
    // `button.image().growX().pad(4).color(Pal.gray)`: the 3 px `whiteui` rule.
    // code-instantiated: divider between the map name and the preview.
    let mut rule_pad = MarginContainer::new_alloc();
    rule_pad.set_mouse_filter(MouseFilter::IGNORE);
    for side in ["left", "right", "top", "bottom"] {
        rule_pad.add_theme_constant_override(&format!("margin_{side}"), 4);
    }
    let mut rule = ColorRect::new_alloc();
    rule.set_color(PAL_GRAY);
    rule.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    rule.set_mouse_filter(MouseFilter::IGNORE);
    rule_pad.add_child(&rule);
    column.add_child(&rule_pad);
    // `button.stack(new Image(map.safeTexture()).setScaling(Scaling.fit),
    // new BorderImage(map.safeTexture()).setScaling(Scaling.fit)).size(mapsize - 20f)`.
    let preview = maplist_build_preview(&map_name, MAP_SIZE - 20.0, 4.0);
    column.add_child(&preview);
    if state.borrow().display_type {
        // `button.add(map.custom ? "@custom" : map.workshop ? "@workshop" :
        // map.mod != null ? "[lightgray]" + modName : "@builtin")
        // .color(Color.gray).padTop(3f)`: the source label row.
        // code-instantiated: the source label row is data-driven per map.
        let mut type_label = Label::new_alloc();
        type_label.set_text(&maplist_source_label(map));
        type_label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        type_label.add_theme_color_override("font_color", styles::ARC_GRAY);
        type_label.set_mouse_filter(MouseFilter::IGNORE);
        // code-instantiated: `padTop(3f)` spacer above the source label.
        let mut spacer = Control::new_alloc();
        spacer.set_custom_minimum_size(Vector2::new(0.0, 3.0));
        spacer.set_mouse_filter(MouseFilter::IGNORE);
        column.add_child(&spacer);
        column.add_child(&type_label);
    }
    // A `Button` is not a container: size the card to its column once the theme
    // fonts resolve (the card height is the content height, like Arc's
    // `TextButton` pref size).
    let inset_for_ready = inset.clone();
    let tile_for_ready = tile.clone();
    let callable = Callable::from_fn("maplist_tile_ready", move |_args: &[&Variant]| {
        let inset = inset_for_ready.clone();
        let mut tile = tile_for_ready.clone();
        let height = inset.get_combined_minimum_size().y;
        let mut minimum = tile.get_custom_minimum_size();
        minimum.y = height;
        tile.set_custom_minimum_size(minimum);
    });
    tile.connect("ready", &callable);
    tile
}

/// `button.table(t -> { t.left(); for(Gamemode mode : Gamemode.all) ... })`:
/// a 16 px icon (pad 4) per valid mode whose icon exists, or one empty 16 px
/// cell when none.
pub(crate) fn maplist_build_mode_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    map: &VarDictionary,
) -> Gd<Control> {
    // code-instantiated: the mode-icon row reflects the map's valid Gamemodes.
    let mut pad = MarginContainer::new_alloc();
    pad.set_mouse_filter(MouseFilter::IGNORE);
    pad.set_h_size_flags(SizeFlags::SHRINK_BEGIN);
    for side in ["left", "right", "top", "bottom"] {
        pad.add_theme_constant_override(&format!("margin_{side}"), 4);
    }
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 8);
    row.set_mouse_filter(MouseFilter::IGNORE);
    pad.add_child(&row);
    for mode in GAMEMODES {
        let Some((_, icon)) = MODE_ICONS.iter().find(|(name, _)| *name == mode) else {
            continue;
        };
        if !maplist_mode_valid(dialog, state, map, mode) {
            continue;
        }
        row.add_child(&maplist_icon_cell(icon, 16.0));
    }
    if row.get_child_count() == 0 {
        // code-instantiated: `t.add().size(16f).pad(4f)` placeholder cell.
        let mut empty = Control::new_alloc();
        empty.set_custom_minimum_size(Vector2::new(16.0, 16.0));
        empty.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&empty);
    }
    let control: Gd<Control> = pad.upcast();
    control
}

/// A fixed-size icon cell (`t.image(icon).size(16f)`) holding an icon glyph.
pub(crate) fn maplist_icon_cell(icon_name: &str, extent: f32) -> Gd<Control> {
    // code-instantiated: icon cells are data-driven (one per valid gamemode).
    let mut cell = Control::new_alloc();
    cell.set_custom_minimum_size(Vector2::new(extent, extent));
    cell.set_mouse_filter(MouseFilter::IGNORE);
    let mut glyph = glyph_control(icon_name, extent, Color::WHITE);
    glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    glyph.set_h_grow_direction(GrowDirection::BOTH);
    glyph.set_v_grow_direction(GrowDirection::BOTH);
    glyph.set_mouse_filter(MouseFilter::IGNORE);
    cell.add_child(&glyph);
    let control: Gd<Control> = cell.upcast();
    control
}

/// The fitted map preview (`Scaling.fit` inside a `size x size` cell with a
/// `BorderImage` stroke), or the `nomap` placeholder until the batched
/// generation lands (`map.safeTexture()`).
pub(crate) fn maplist_build_preview(map_name: &str, extent: f32, thickness: f32) -> Gd<Control> {
    if let Some(texture) = preview_texture(map_name) {
        // code-instantiated: the preview texture comes from the map registry.
        let mut image = border_preview(&texture, Vector2::new(extent, extent), thickness);
        image.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        image.set_mouse_filter(MouseFilter::IGNORE);
        return image;
    }
    // code-instantiated: the `nomap` placeholder until map textures exist.
    let mut placeholder = PanelContainer::new_alloc();
    placeholder.set_theme_type_variation("smallPane");
    placeholder.set_custom_minimum_size(Vector2::new(extent, extent));
    placeholder.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    placeholder.set_mouse_filter(MouseFilter::IGNORE);
    let glyph = glyph_control("map", 48.0, Color::WHITE);
    placeholder.add_child(&glyph);
    let control: Gd<Control> = placeholder.upcast();
    control
}

/// `_source_label(map)`: `@custom` / `@workshop` / the mod name / `@builtin`.
pub(crate) fn maplist_source_label(map: &VarDictionary) -> GString {
    if row_bool(map, "custom") {
        return kit::t("@custom");
    }
    if row_bool(map, "workshop") {
        return kit::t("@workshop");
    }
    if row_bool(map, "mod") {
        return gs(row_str(map, "mod_name"));
    }
    kit::t("@builtin")
}

/// `showMap(map)`: the per-controller override
/// ([`MapListState::show_map`]) or `CustomGameDialog`'s route.
pub(crate) fn maplist_show_map_for(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    map: &VarDictionary,
) {
    let hook = state.borrow().show_map.clone();
    if let Some(hook) = hook {
        hook(dialog, map);
        return;
    }
    maplist_show_map(dialog, map);
}

/// `showMap(map)`: `CustomGameDialog` emits `map_chosen` then opens `map_play`.
pub(crate) fn maplist_show_map(dialog: &mut MindDialog, map: &VarDictionary) {
    let map_name = row_str(map, "name");
    dialog
        .base_mut()
        .emit_signal("map_chosen", &[gs(map_name.as_str()).to_variant()]);
    let mut context = VarDictionary::new();
    context.set("map", &gs(map_name));
    open_dialog("map_play", &context);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn map_sort_keys_follow_map_priority() {
        // Godot-FFI-backed lookups/types need the engine; plain `cargo test`
        // runs without it (the headless oracles cover the engine path).
        if !godot::sys::is_godot_initialized() {
            return;
        }
        let a = MapSortKey {
            row: VarDictionary::new(),
            index: 1,
            name: String::from("alpha"),
            custom: true,
            modified: 10,
        };
        let b = MapSortKey {
            row: VarDictionary::new(),
            index: 0,
            name: String::from("beta"),
            custom: false,
            modified: 20,
        };
        assert_eq!(maplist_compare("name", &a, &b), Ordering::Less);
        assert_eq!(maplist_compare("builtin", &a, &b), Ordering::Greater);
        // `recent` prefers the newer (larger) `lastModified`.
        assert_eq!(maplist_compare("recent", &a, &b), Ordering::Greater);
        // Equal-key rows fall back to the registry order.
        assert_eq!(maplist_compare("name", &a, &a), Ordering::Equal);
    }
}
