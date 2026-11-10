// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/ResearchDialog.java,
//         core/src/mindustry/ui/dialogs/SchematicsDialog.java,
//         core/src/mindustry/ui/dialogs/LoadoutDialog.java,
//         core/src/mindustry/ui/dialogs/LaunchLoadoutDialog.java,
//         core/src/mindustry/ui/dialogs/PlanetDialog.java (`showStats`),
//         core/src/mindustry/ui/dialogs/CustomGameDialog.java,
//         core/src/mindustry/ui/dialogs/MapListDialog.java,
//         core/src/mindustry/ui/dialogs/MapPlayDialog.java.
//
//! Campaign dialog controllers, batch 2: research, schematics, loadout,
//! launch loadout, sector stats, custom game and map play.
//!
//! Each controller is a native Rust controller that builds the data-driven
//! content of one `.tscn` dialog shell. The static structure lives in the
//! scenes; every row here is rebuilt from the `MindCampaign` read models
//! (`campaign_views_json`, `research_tree_json`, `launch_dialog_json`) and the
//! `MindPreview` map registry, so the controllers stay UI-only. The tech-tree
//! canvas, the `@globalitems` pool and the schematic previews are the native
//! kit widgets (`MindResearchView`, `MindResearchItemsDisplay`,
//! `MindSchematicImage`).

use std::cell::RefCell;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use godot::builtin::{Side, VarArray, VarDictionary, VariantType};
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{GrowDirection, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::file_dialog::{Access, FileMode};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_edit::LineWrappingMode;
use godot::classes::text_server::{AutowrapMode, OverrunBehavior};
use godot::classes::{
    Button, CanvasItem, CenterContainer, ColorRect, Control, DisplayServer, FileDialog, Font,
    FontVariation, HBoxContainer, Json, Label, LineEdit, MarginContainer, Node, PanelContainer,
    RichTextLabel, StyleBox, StyleBoxEmpty, StyleBoxFlat, StyleBoxTexture, TextEdit, Texture2D,
    TextureRect, VBoxContainer,
};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;
use mind_core::game::schematic::Schematic;
use mind_core::game::schematics;
use mind_core::io::{FileSystem, NativeFs, Paths};
use mind_core::util::strip_colors;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::factory;
use super::super::kit::widgets::research_items_display::MindResearchItemsDisplay;
use super::super::kit::widgets::research_view::MindResearchView;
use super::super::kit::widgets::schematic_image::MindSchematicImage;
use super::super::kit::{self, MindLabel, MindScroll, MindTable, styles};
use crate::MindSimHost;
use crate::editor::MindPreview;

use super::campaign::util::*;
pub(crate) use super::campaign::util::{Deferred, install_frame_tick};

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "custom" => Some(Box::new(CustomGameDialog::default())),
        "map_play" => Some(Box::new(MapPlayDialog::default())),
        _ => None,
    }
}

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

/// `CustomGameDialog extends MapListDialog` with `displayType = false`.
pub struct CustomGameDialog {
    state: Rc<RefCell<MapListState>>,
}

impl Default for CustomGameDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(MapListState::new())),
        }
    }
}

impl DialogController for CustomGameDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@customgame");
        dialog.should_pause = false;
        dialog.enable_full_dialog(true);
        // `makeButtonOverlay()` (`MapListDialog.java:103`).
        dialog.apply_button_overlay(true);
        maplist_load_settings(dialog, &self.state);
        maplist_build(dialog, &self.state);
        let deferred = {
            let st = self.state.borrow();
            st.deferred.clone()
        };
        install_frame_tick(dialog, "custom", deferred, || {});
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        maplist_load_settings(dialog, &self.state);
        maplist_refresh_registry(dialog, &self.state);
        // `setup`: `searchString = null` and a fresh (empty) search field.
        self.state.borrow_mut().search = String::new();
        if let Some(mut field) = self.state.borrow().field.clone() {
            field.set_text("");
        }
        maplist_rebuild(dialog, &self.state);
        // `MapListDialog.show`: desktop focuses the search field.
        if let Some(mut field) = self.state.borrow().field.clone() {
            field.call_deferred("grab_focus", &[]);
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
        .map(|value| text_of(&value))
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

// ---------------------------------------------------------------------------
// MapPlayDialog
// ---------------------------------------------------------------------------

/// Visible `Gamemode` names in declaration order (`Gamemode.all` minus editor).
const MAP_PLAY_MODES: [&str; 4] = ["survival", "sandbox", "attack", "pvp"];
/// `MapPlayDialog` mode-button size (`size(140f, mobile ? 44f : 54f)`).
const MODE_BUTTON: Vector2 = Vector2::new(140.0, 54.0);
/// `size(mobile && !portrait ? 150f : 250f)`.
const PREVIEW_SIZE: Vector2 = Vector2::new(250.0, 250.0);
/// `buttons.button(...).size(210f, 64f)` (`addCloseButton` + `@play`).
const MAP_PLAY_ACTION_BUTTON: Vector2 = Vector2::new(210.0, 64.0);

/// Controller state for the reused gamemode-selection dialog node.
#[derive(Default)]
struct MapPlayState {
    map_name: String,
    mode: String,
    /// Working rules JSON (`MapPlayDialog.rules`); empty = the map's own rules.
    rules_json: String,
    /// `MapPlayDialog.show(map, playtesting)`: the editor playtest route.
    playtest: bool,
    mode_buttons: Vec<Gd<Button>>,
    button_modes: Vec<String>,
}

impl MapPlayState {
    fn new() -> Self {
        Self {
            mode: String::from("survival"),
            ..Self::default()
        }
    }
}

/// Gamemode selection, the mode help list, Customize Rules, the map preview and
/// the high score (`MapPlayDialog`).
pub struct MapPlayDialog {
    state: Rc<RefCell<MapPlayState>>,
}

impl Default for MapPlayDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(MapPlayState::new())),
        }
    }
}

impl DialogController for MapPlayDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        map_play_floating_layout(dialog);
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        let map_name = dialog.context_string("map").to_string();
        let requested = dialog.context_string("mode").to_string();
        let rules = dialog.context_string("rules").to_string();
        let playtest = dialog
            .context_value("playtest")
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        let mut st = self.state.borrow_mut();
        st.map_name = map_name;
        if MAP_PLAY_MODES.contains(&requested.as_str()) {
            st.mode = requested;
        }
        st.rules_json = rules;
        st.playtest = playtest;
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let mut st = self.state.borrow_mut();
        if st.map_name.is_empty() {
            st.map_name = dialog.context_string("map").to_string();
        }
        drop(st);
        map_play_build(dialog, &self.state);
    }
}

/// `super(""); setFillParent(false)` with `Styles.defaultDialog`
/// (`background = windowEmpty`, `stageBackground = black9`): the content floats
/// centred over the dim with no boxed panel and no scrolling body.
fn map_play_floating_layout(dialog: &mut MindDialog) {
    if let Some(mut panel) = dialog.panel() {
        panel.set_theme_type_variation("none");
        panel.set_custom_minimum_size(Vector2::ZERO);
        // The floating table still carries `windowEmpty`'s 4 px inset above the
        // (unpadded) title row, and under the buttons the button's own `pad(3)`
        // plus the `buttons` cell's `pad(3)` (`Dialog.java:208-214`).
        let mut inset = StyleBoxEmpty::new_gd();
        inset.set_content_margin(Side::TOP, 4.0);
        inset.set_content_margin(Side::BOTTOM, 4.0 + 3.0 + 3.0);
        panel.add_theme_stylebox_override("panel", &inset);
    }
    if let Some(mut body) = dialog.body() {
        // The body reports its content height so nothing is clipped/scrolled.
        body.set_vertical_scroll_mode(ScrollMode::DISABLED);
        body.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    }
    if let Some(mut buttons) = dialog.buttons_row() {
        buttons.add_theme_constant_override("separation", 8);
    }
    dialog.title_color = styles::ACCENT;
}

fn map_play_build(dialog: &mut MindDialog, state: &Rc<RefCell<MapPlayState>>) {
    let (map_name, mut mode, mut rules_json) = {
        let st = state.borrow();
        (st.map_name.clone(), st.mode.clone(), st.rules_json.clone())
    };
    let title = if map_name.is_empty() {
        kit::t("@map.play").to_string()
    } else {
        map_name.clone()
    };
    dialog.set_title_literal(&title);
    // `cont.clearChildren()` (`MapPlayDialog.java:45`): the body is rebuilt per
    // shown map.
    dialog.clear_content();
    let modes = map_play_mode_validity(dialog, &map_name);
    // `if(!selectedGamemode.valid(map)) selectedGamemode = first valid || survival`.
    if !row_bool_or(&modes, &mode, true) {
        let mut picked = String::new();
        for candidate in MAP_PLAY_MODES {
            if row_bool_or(&modes, candidate, false) {
                picked = candidate.to_string();
                break;
            }
        }
        mode = if picked.is_empty() {
            String::from("survival")
        } else {
            picked
        };
    }
    if rules_json.is_empty() {
        rules_json = map_play_mode_rules(dialog, &map_name, &mode);
    }
    {
        let mut st = state.borrow_mut();
        st.mode = mode.clone();
        st.rules_json = rules_json;
    }
    let mut root = dialog.content_table();
    // `Table selmode`: `add("@level.mode").colspan(2)`, then the `Tex.button`
    // mode table and the `?` button on one row (`MapPlayDialog.java:56-71`).
    let mut selmode = factory::table();
    let mode_label = factory::label(&kit::t("@level.mode").to_string());
    selmode.bind_mut().add(&mode_label).colspan(2);
    selmode.bind_mut().row();
    // code-instantiated: `selmode.table(Tex.button, modes -> ...)` frame.
    let mut mode_frame = PanelContainer::new_alloc();
    let mut frame_box = MindDialog::pane_box(Color::BLACK, OUTLINE_GRAY, 4.0, 2.0, -1.0);
    // `Tex.button` pads its content 12 px (the 9-patch splits): the Java mode
    // box is 304x132 around the 2x2 140x54 buttons (java/mapplay-caldera.png).
    for side in [Side::LEFT, Side::RIGHT, Side::TOP, Side::BOTTOM] {
        frame_box.set_content_margin(side, 12.0);
    }
    mode_frame.add_theme_stylebox_override("panel", &frame_box);
    let mut mode_box = factory::table();
    mode_frame.add_child(&mode_box);
    state.borrow_mut().mode_buttons.clear();
    state.borrow_mut().button_modes.clear();
    for (index, candidate) in MAP_PLAY_MODES.iter().enumerate() {
        // code-instantiated: one toggle per visible Gamemode (`Gamemode.all`).
        let mut button = factory::button(&kit::t(&format!("@mode.{candidate}.name")).to_string());
        button.set_theme_type_variation("flatToggleMenut");
        apply_flat_toggle_menu(&mut button);
        button.set_toggle_mode(true);
        button.set_pressed(*candidate == mode);
        button.set_disabled(!row_bool_or(&modes, candidate, true));
        button.set_custom_minimum_size(MODE_BUTTON);
        let target = dialog.to_gd();
        let state_for_mode = state.clone();
        let mode_name = candidate.to_string();
        let callable = Callable::from_fn("map_play_select_mode", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            map_play_select_mode(&mut bound, &state_for_mode, &mode_name);
        });
        button.connect("pressed", &callable);
        mode_box
            .bind_mut()
            .add(&button)
            .width(MODE_BUTTON.x)
            .height(MODE_BUTTON.y);
        if index % 2 == 1 {
            mode_box.bind_mut().row();
        }
        state.borrow_mut().mode_buttons.push(button);
        state.borrow_mut().button_modes.push(candidate.to_string());
    }
    selmode.bind_mut().add(&mode_frame);
    // `selmode.button("?", this::displayGameModeHelp).width(50f).fillY().padLeft(18f)`.
    let mut help = factory::button("?");
    dialog.apply_pane_button(&mut help, OUTLINE_GRAY, 4.0);
    help.set_custom_minimum_size(Vector2::new(50.0, 0.0));
    let target = dialog.to_gd();
    let callable = Callable::from_fn("map_play_help", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        map_play_show_mode_help(&mut bound);
    });
    help.connect("pressed", &callable);
    selmode
        .bind_mut()
        .add(&help)
        .width(50.0)
        .fill_y()
        .pad_left(18.0);
    root.bind_mut().add(&selmode).expand_x();
    root.bind_mut().row();
    // `cont.button("@customize", Icon.settings, ...).height(50f).width(230)`.
    let mut customize = factory::button("");
    dialog.apply_pane_button(&mut customize, OUTLINE_GRAY, 4.0);
    let glyph: Gd<Control> = glyph_control("settings", 24.0, Color::WHITE).upcast();
    let customize_text = kit::t("@customize").to_string();
    dialog.build_button_content(&mut customize, Some(glyph), &customize_text);
    customize.set_custom_minimum_size(Vector2::new(230.0, 50.0));
    let target = dialog.to_gd();
    let state_for_rules = state.clone();
    let callable = Callable::from_fn("map_play_open_rules", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        map_play_open_rules(&mut bound, &state_for_rules);
    });
    customize.connect("pressed", &callable);
    root.bind_mut().add(&customize).width(230.0).height(50.0);
    root.bind_mut().row();
    // `cont.add(new BorderImage(map.safeTexture(), 3f)).size(250f)
    // .get().setScaling(Scaling.fit)`: the whole preview, fitted.
    if let Some(texture) = preview_texture(&map_name) {
        // code-instantiated: the preview texture comes from the map registry.
        let mut image = border_preview(&texture, PREVIEW_SIZE, 3.0);
        image.set_mouse_filter(MouseFilter::IGNORE);
        root.bind_mut()
            .add(&image)
            .width(PREVIEW_SIZE.x)
            .height(PREVIEW_SIZE.y);
    }
    // `if(Gamemode.survival.valid(map)) cont.label(level.highscore).pad(3f)`.
    if row_bool_or(&modes, "survival", false) {
        root.bind_mut().row();
        let label = factory::label(&map_play_high_score(&map_name));
        root.bind_mut().add(&label).pad(3.0);
    }
    dialog.clear_buttons();
    // `addCloseButton()` (210x64) then `buttons.button("@play", Icon.play, ...)
    // .size(210f, 64f)`.
    let close = dialog.base().callable("close_pressed");
    let back_text = kit::t("@back").to_string();
    map_play_action_button(dialog, &back_text, "left", close);
    let target = dialog.to_gd();
    let state_for_play = state.clone();
    let callable = Callable::from_fn("map_play_start", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        map_play_start(&mut bound, &state_for_play);
    });
    let play_text = kit::t("@play").to_string();
    map_play_action_button(dialog, &play_text, "play", callable);
}

/// A 210x64 `Tex.button` action button with an icon (`BaseDialog` buttons row).
fn map_play_action_button(
    dialog: &mut MindDialog,
    text_value: &str,
    icon_name: &str,
    listener: Callable,
) {
    // code-instantiated: the button row is rebuilt per shown map.
    let mut button = factory::button("");
    button.set_custom_minimum_size(MAP_PLAY_ACTION_BUTTON);
    dialog.apply_pane_button(&mut button, OUTLINE_GRAY, 4.0);
    let glyph: Gd<Control> = glyph_control(icon_name, 24.0, Color::WHITE).upcast();
    dialog.build_button_content(&mut button, Some(glyph), text_value);
    if listener.is_valid() {
        button.connect("pressed", &listener);
    }
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&button);
    }
}

/// `Gamemode.valid(map)` per visible mode (`MindPreview.map_modes`).
fn map_play_mode_validity(dialog: &mut MindDialog, map_name: &str) -> VarDictionary {
    let _ = dialog;
    let Some(mut preview) = preview_node() else {
        return VarDictionary::new();
    };
    preview.bind_mut().map_modes(gs(map_name))
}

/// `map.applyRules(mode)` serialized (`MindPreview.map_rules_json`).
fn map_play_mode_rules(dialog: &mut MindDialog, map_name: &str, mode: &str) -> String {
    let _ = dialog;
    let Some(mut preview) = preview_node() else {
        return String::new();
    };
    preview
        .bind_mut()
        .map_rules_json(gs(map_name), gs(mode))
        .to_string()
}

/// `mode.toString()` + the selected index, refreshing the rules.
fn map_play_select_mode(dialog: &mut MindDialog, state: &Rc<RefCell<MapPlayState>>, mode: &str) {
    let (buttons, modes, map_name) = {
        let st = state.borrow();
        (
            st.mode_buttons.clone(),
            st.button_modes.clone(),
            st.map_name.clone(),
        )
    };
    for (index, button) in buttons.iter().enumerate() {
        let selected = modes
            .get(index)
            .map(|candidate| candidate == mode)
            .unwrap_or(false);
        set_toggle(button, selected);
    }
    // `rules = map.applyRules(mode)` upstream.
    let rules = map_play_mode_rules(dialog, &map_name, mode);
    let mut st = state.borrow_mut();
    st.mode = mode.to_string();
    st.rules_json = rules;
}

/// `displayGameModeHelp` (`MapPlayDialog.java:91-112`): every visible mode as
/// `[accent]<name>:[] [lightgray]<description>`, width 400, `@ok` 110x50.
fn map_play_show_mode_help(dialog: &mut MindDialog) {
    let _ = dialog;
    let mut lines: Vec<String> = Vec::new();
    for mode in MAP_PLAY_MODES {
        lines.push(format!(
            "[accent]{}:[] [lightgray]{}",
            kit::t(&format!("@mode.{mode}.name")),
            kit::t(&format!("@mode.{mode}.description"))
        ));
    }
    let body = lines.join("\n");
    let _ = ui_call(
        "show_text",
        &[
            kit::t("@mode.help.title").to_variant(),
            kit::markup_text(&body).to_variant(),
        ],
    );
}

/// `CustomRulesDialog(true).show(rules, () -> rules = map.applyRules(mode))`:
/// the dialog edits the working rules; `rules_edited` replaces them.
fn map_play_open_rules(dialog: &mut MindDialog, state: &Rc<RefCell<MapPlayState>>) {
    let (rules_json, mode, map_name) = {
        let st = state.borrow();
        (st.rules_json.clone(), st.mode.clone(), st.map_name.clone())
    };
    let current = parse_object(&rules_json);
    let reset = parse_object(&map_play_mode_rules(dialog, &map_name, &mode));
    let mut context = VarDictionary::new();
    context.set("rules", &current.to_variant());
    context.set("reset", &reset.to_variant());
    context.set("show_rule_edit", &false.to_variant());
    open_dialog("custom_rules", &context);
    let Some(rules_dialog) = dialog_node("custom_rules") else {
        return;
    };
    if !rules_dialog.has_signal("rules_edited") {
        return;
    }
    let state_for_rules = state.clone();
    let callable = Callable::from_fn("map_play_rules_edited", move |args: &[&Variant]| {
        let json = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        state_for_rules.borrow_mut().rules_json = json;
    });
    connect_once(
        dialog,
        "map_play_rules",
        &rules_dialog,
        "rules_edited",
        &callable,
    );
}

/// `Core.bundle.format("level.highscore", map.getHighScore())`.
fn map_play_high_score(map_name: &str) -> String {
    let default = kit::t("@level.highscore").to_string();
    let Some(preview) = preview_node() else {
        return default;
    };
    let key = preview.bind().map_high_score_key(gs(map_name));
    if key.is_empty() {
        return default;
    }
    let score = setting_get_int(&key.to_string(), 0);
    kit::markup_text(&default.replace("{0}", &score.to_string())).to_string()
}

/// `control.playMap(map, rules, playtesting)` + `ui.custom.hide()`.
fn map_play_start(dialog: &mut MindDialog, state: &Rc<RefCell<MapPlayState>>) {
    let (map_name, mode, playtest, rules_json) = {
        let st = state.borrow();
        (
            st.map_name.clone(),
            st.mode.clone(),
            st.playtest,
            st.rules_json.clone(),
        )
    };
    let mut started = false;
    if !map_name.is_empty()
        && let Some(mut campaign) = dialog.campaign_node()
        && campaign.has_method("start_map_mode")
    {
        // `start_map_mode` loads the world, applies the gamemode + custom rules
        // and starts the session in one call (`Control.playMap`).
        started = campaign
            .call(
                "start_map_mode",
                &[
                    gs(map_name).to_variant(),
                    gs(mode).to_variant(),
                    playtest.to_variant(),
                    gs(rules_json).to_variant(),
                ],
            )
            .try_to::<bool>()
            .unwrap_or(false);
    }
    map_play_close_stack(dialog);
    if !started && playtest {
        // `Control.playMap`: "booted out of map, resume editing"
        // (`Control.java:422-428`): the editor still holds the map the playtest
        // just saved, so re-show it over the error.
        if let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
            && shell.has_method("show_dialog")
        {
            shell.call("show_dialog", &[]);
        }
        return;
    }
    if started {
        // `MapEditorDialog.playtest`'s `playListener = this::hide`: a playtest
        // returns to the editor, so hide the shell while the map plays.
        if playtest
            && let Some(mut editor) = kit::node("/root/Spine/Ui/EditorDialog")
            && editor.has_method("hide_dialog")
        {
            editor.call("hide_dialog", &[]);
        }
        if let Some(mut root) = ui_root()
            && root.has_method("set_menu_visible")
        {
            root.call("set_menu_visible", &[false.to_variant()]);
        }
    }
}

fn map_play_close_stack(dialog: &mut MindDialog) {
    if kit::ui().is_none() {
        dialog.hide_dialog();
        return;
    }
    close_dialog("map_play");
    close_dialog("custom");
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
