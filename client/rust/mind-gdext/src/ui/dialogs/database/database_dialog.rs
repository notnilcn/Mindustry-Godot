// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The database dialog (`database`): the search row, planet tab rail and category/tag content grid.
//! Upstream: core/src/mindustry/ui/dialogs/DatabaseDialog.java

use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, ButtonGroup, ColorRect, Control, GridContainer, HBoxContainer, LineEdit,
    MarginContainer, RichTextLabel, StyleBoxEmpty, StyleBoxFlat, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindScroll, MindTable, styles};

use crate::ui::dialogs::settings::util::*;

pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "database" => Some(Box::new(DatabaseController::default())),
        _ => None,
    }
}

/// `Planets.sun` content name (`DatabaseDialog.tab`'s all-content default).
const DATABASE_ALL_TAB: &str = "sun";
/// `Vars.iconMed` (grid cell size).
const DATABASE_ICON_MED: f32 = 32.0;
/// `Styles.defaultPane` bar width (`MindThemeBuilder.SCROLL_BAR_WIDTH`): how
/// much narrower than its pref Arc lays out `all`.
const DATABASE_ARC_BAR_SQUEEZE: i32 = 24;
/// `Pal.gray` (#454545): tag headers and lock/cancel overlays.
const DATABASE_PAL_GRAY: Color = Color::from_rgba8(0x45, 0x45, 0x45, 0xff);
/// `Color.scarlet` at the banned overlay's alpha (`Tmp.c1.set(Color.scarlet).a(0.6f)`).
const DATABASE_SCARLET: Color = Color::from_rgba(1.0, 52.0 / 255.0, 28.0 / 255.0, 0.6);

/// Meta key recording the connected `size_changed` rebuild hook.
const DATABASE_RESIZE_META: &str = "mind_database_resize_bound";

/// Port-only Database tab over `Achievement` (GAPS #68 UI half). Upstream has
/// no achievement dialog (Steam's overlay showed them), so there is no Java
/// label to port; the rail uses a star glyph and the tooltip names the table.
const DATABASE_ACHIEVEMENTS_TAB: &str = "achievements";
/// Port-only Database tab over `SStat` (GAPS #68 UI half).
const DATABASE_STATISTICS_TAB: &str = "statistics";
/// Glyph for the achievements tab (`Fonts.icon` star).
const DATABASE_ACHIEVEMENTS_ICON: &str = "star";
/// Glyph for the statistics tab (`Fonts.icon` chart-bar).
const DATABASE_STATISTICS_ICON: &str = "chart-bar";

/// Rebuild handles shared with the search/tab/resize callbacks.
#[derive(Default)]
struct DatabaseState {
    tabs: Option<Gd<GridContainer>>,
    grid: Option<Gd<VBoxContainer>>,
    search: Option<Gd<LineEdit>>,
    view: VarDictionary,
    tab: String,
    /// Port-only tabs (`achievements`/`statistics`) with a non-empty row set.
    special: Vec<String>,
}

/// `@database` — search field, planet tabs and the
/// `databaseCategory`/`databaseTag` content grid over the live
/// `MindCampaign.database_json` model.
pub struct DatabaseController {
    state: Rc<RefCell<DatabaseState>>,
}

impl Default for DatabaseController {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(DatabaseState {
                tab: DATABASE_ALL_TAB.to_string(),
                ..DatabaseState::default()
            })),
        }
    }
}

impl DialogController for DatabaseController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        // `cont.top()` (`DatabaseDialog.java:57`): the body sticks to the top.
        dialog.apply_cont_top(true);
        dialog.set_title_key("@database");
        dialog.should_pause = true;
        // Java `BaseDialog`: the `cont.pane(all)` grid fills the viewport.
        dialog.fill_panel();
        dialog.clear_content();
        let state = self.state.clone();
        database_build(dialog, &state);
        dialog.add_close_button(210.0);
        database_connect_resize(dialog, &state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let state = self.state.clone();
        database_load_view(dialog, &state);
    }
}

/// `_build`: the search row, the scrolling `all` table (tab rail + grid) and
/// its `size_changed` rebuild hook.
fn database_build(dialog: &mut MindDialog, state: &Rc<RefCell<DatabaseState>>) {
    let mut table = dialog.content_table();
    {
        let mut control: Gd<Control> = table.clone().upcast();
        control.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    }
    // `cont.table(s -> { s.image(Icon.zoom).padRight(8); search = s.field(..) })`.
    // code-instantiated: the search row is part of the dialog body built in code.
    let mut search_row = HBoxContainer::new_alloc();
    search_row.add_theme_constant_override("separation", 8);
    search_row.add_child(&factory::glyph_label("zoom", 30, Color::WHITE));
    let mut search = factory::field(&kit::t("@players.search").to_string());
    search.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let target = dialog.to_gd();
        let state_for_search = state.clone();
        let callable = Callable::from_fn("database_search", move |_args: &[&Variant]| {
            if !target.is_instance_valid() {
                return;
            }
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            database_rebuild(&mut bound, &state_for_search);
        });
        search.connect("text_changed", &callable);
    }
    search_row.add_child(&search);
    table.bind_mut().add(&search_row).grow_x().pad_bottom(4.0);
    table.bind_mut().row();

    // `cont.pane(all).scrollX(false)` (`DatabaseDialog.java:64`): only `all`
    // scrolls; the shell's Body stops scrolling and the pane takes the height
    // down to the button row.
    if let Some(mut body) = dialog.body() {
        body.set_vertical_scroll_mode(ScrollMode::DISABLED);
    }
    // code-instantiated: the pane wraps the runtime-built `all` table.
    let mut pane = MindScroll::new_alloc();
    pane.bind_mut().set_scrolling_disabled(true, false);
    // code-instantiated: `all` holds the live tab rail and category grid.
    let mut all = MindTable::new_alloc();
    all.bind_mut().set_table_top(true);
    {
        let control: Gd<Control> = all.clone().upcast();
        pane.add_child(&control);
    }
    table.bind_mut().add(&pane).grow_y();

    // code-instantiated: the tab rail is the data-driven planet tab list
    // (`t.button(..., Styles.clearNoneTogglei, iconMed, ...).size(50f)`, 10 per
    // row).
    let mut tabs = GridContainer::new_alloc();
    tabs.set_columns(10);
    tabs.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    tabs.add_theme_constant_override("h_separation", 0);
    tabs.add_theme_constant_override("v_separation", 0);
    all.bind_mut().add(&tabs).pad_left(20.0).pad_right(30.0);
    all.bind_mut().row();

    // `all.margin(20).marginTop(0f).marginRight(30f)`: Godot's pane adds the
    // scroll bar to its minimum width, so give the bar's width back here (the
    // category rule takes it as extra right padding).
    // code-instantiated: the category/tag grid is live content, not scene state.
    let mut margin = MarginContainer::new_alloc();
    margin.add_theme_constant_override("margin_left", 20);
    margin.add_theme_constant_override("margin_right", 30 - DATABASE_ARC_BAR_SQUEEZE);
    margin.add_theme_constant_override("margin_top", 0);
    margin.add_theme_constant_override("margin_bottom", 20);
    let mut grid = VBoxContainer::new_alloc();
    grid.add_theme_constant_override("separation", 0);
    margin.add_child(&grid);
    all.bind_mut().add(&margin).grow_x();

    {
        let mut st = state.borrow_mut();
        st.tabs = Some(tabs);
        st.grid = Some(grid);
        st.search = Some(search);
    }
    database_load_view(dialog, state);
}

/// The dialog's viewport `size_changed` -> grid rebuild (`onResize(this::rebuild)`).
fn database_connect_resize(dialog: &mut MindDialog, state: &Rc<RefCell<DatabaseState>>) {
    let mut node = dialog.to_gd();
    if node.has_meta(DATABASE_RESIZE_META) {
        return;
    }
    let Some(mut viewport) = dialog.base().get_viewport() else {
        return;
    };
    let target = node.clone();
    let state_for_resize = state.clone();
    let callable = Callable::from_fn("database_resize", move |_args: &[&Variant]| {
        if !target.is_instance_valid() {
            return;
        }
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        database_rebuild(&mut bound, &state_for_resize);
    });
    viewport.connect("size_changed", &callable);
    node.set_meta(DATABASE_RESIZE_META, &true.to_variant());
}

/// The `{tabs: [], categories: []}` fallback model.
fn database_empty_view() -> VarDictionary {
    let mut view = VarDictionary::new();
    view.set("tabs", &VarArray::new());
    view.set("categories", &VarArray::new());
    view
}

/// Reads the live database model from `MindCampaign` and repaints the rail and
/// the grid.
///
/// The two port-only tabs (`achievements`/`statistics`, GAPS #68 UI half) are
/// appended when their row set is non-empty: a profile whose settings store
/// cannot be read shows no empty tab, and no rows are ever invented.
fn database_load_view(dialog: &mut MindDialog, state: &Rc<RefCell<DatabaseState>>) {
    let mut view = dialog.campaign_json("database_json", &[]);
    if view.is_empty() {
        view = database_empty_view();
    }
    let mut tab = state.borrow().tab.clone();
    let special = database_special_tabs(dialog);
    let tabs = dict_array(&view, "tabs");
    let mut names: Vec<String> = tabs
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .map(|row| dict_text(&row, "name"))
        .collect();
    names.extend(special.iter().cloned());
    if !tabs.is_empty() {
        // `DatabaseDialog.shown`: in a campaign the active tab follows the
        // played planet (`state.getPlanet()`) when the tab list has it.
        let mut current_planet = String::new();
        if let Some(mut campaign) = dialog.campaign_node()
            && campaign.has_method("get_sector_state")
        {
            let sector_state = campaign.call("get_sector_state", &[]).to::<VarDictionary>();
            if dict_bool(&sector_state, "campaign") {
                current_planet = dict_text(&sector_state, "planet");
            }
        }
        if !current_planet.is_empty() && names.contains(&current_planet) {
            tab = current_planet;
        } else if !names.contains(&tab) {
            tab = tabs
                .get(0)
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .map(|row| dict_text(&row, "name"))
                .unwrap_or_else(|| DATABASE_ALL_TAB.to_string());
        }
    }
    {
        let mut st = state.borrow_mut();
        st.view = view;
        st.tab = tab;
        st.special = special;
    }
    let node = dialog.to_gd();
    database_build_tabs(&node, state);
    database_rebuild(dialog, state);
}

/// The port-only tabs with rows: `achievements`/`statistics` (GAPS #68 UI
/// half), in tab order.
fn database_special_tabs(dialog: &MindDialog) -> Vec<String> {
    let mut tabs = Vec::new();
    for (kind, label) in [
        (DATABASE_ACHIEVEMENTS_TAB, DATABASE_ACHIEVEMENTS_TAB),
        (DATABASE_STATISTICS_TAB, DATABASE_STATISTICS_TAB),
    ] {
        if database_table_rows(dialog, label) > 0 {
            tabs.push(kind.to_string());
        }
    }
    if !tabs.is_empty() {
        log::info!("[ui] database tables: {}", tabs.join(", "));
    }
    tabs
}

/// The row JSON of a port-only tab from `MindUi.database_rows_json(kind)`.
///
/// Guarded twice: the `MindUi` autoload may be absent (unit hosts) and the
/// method is new (older .so), so a missing seam answers `[]` — the tab then
/// stays hidden rather than rendering fabricated entries.
fn database_table_rows_json(dialog: &MindDialog, kind: &str) -> String {
    let _ = dialog;
    let Some(mut ui) = kit::ui() else {
        return String::from("[]");
    };
    if !ui.has_method("database_rows_json") {
        return String::from("[]");
    }
    ui.call("database_rows_json", &[GString::from(kind).to_variant()])
        .try_to::<GString>()
        .unwrap_or_default()
        .to_string()
}

/// Parsed achievement rows for the tab.
fn database_achievement_rows(dialog: &MindDialog) -> Vec<kit::rows::AchievementRow> {
    kit::rows::parse_achievements(&database_table_rows_json(dialog, DATABASE_ACHIEVEMENTS_TAB))
}

/// Parsed statistics rows for the tab.
fn database_stat_rows(dialog: &MindDialog) -> Vec<kit::rows::StatRow> {
    kit::rows::parse_stats(&database_table_rows_json(dialog, DATABASE_STATISTICS_TAB))
}

/// Rows of a port-only tab (used for the tab-presence check).
fn database_table_rows(dialog: &MindDialog, kind: &str) -> usize {
    match kind {
        DATABASE_ACHIEVEMENTS_TAB => database_achievement_rows(dialog).len(),
        DATABASE_STATISTICS_TAB => database_stat_rows(dialog).len(),
        _ => 0,
    }
}

/// `_build_tabs`: one 50 px icon toggle per planet tab.
fn database_build_tabs(dialog: &Gd<MindDialog>, state: &Rc<RefCell<DatabaseState>>) {
    let (mut tabs_node, view, tab, special) = {
        let st = state.borrow();
        let Some(tabs) = st.tabs.clone() else {
            return;
        };
        (tabs, st.view.clone(), st.tab.clone(), st.special.clone())
    };
    if !tabs_node.is_instance_valid() {
        return;
    }
    for child in tabs_node.get_children().iter_shared() {
        let mut child = child.clone();
        tabs_node.remove_child(&child);
        child.queue_free();
    }
    let group = ButtonGroup::new_gd();
    let mut rows: Vec<VarDictionary> = dict_array(&view, "tabs")
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect();
    // Port-only tabs over the achievement/stat tables (GAPS #68 UI half): the
    // rail is data-driven, so they join it as synthetic rows with a glyph and
    // the table name as their tooltip (upstream has no labels for these).
    for name in &special {
        let icon = if name == DATABASE_ACHIEVEMENTS_TAB {
            DATABASE_ACHIEVEMENTS_ICON
        } else {
            DATABASE_STATISTICS_ICON
        };
        let mut row = VarDictionary::new();
        row.set("name", &GString::from(name.as_str()));
        row.set("localized", &GString::from(name.as_str()));
        row.set("icon", &GString::from(icon));
        row.set("is_all", &false.to_variant());
        rows.push(row);
    }
    for tab_row in rows {
        let tab_name = dict_text(&tab_row, "name");
        let is_all = dict_bool(&tab_row, "is_all");
        let label = database_tab_label(&tab_row, is_all);
        let color = if is_all {
            Color::WHITE
        } else {
            database_hex_color(&dict_text(&tab_row, "color"))
        };
        let icon_name = dict_text(&tab_row, "icon");
        // code-instantiated: one icon toggle per planet tab (`Icon.eyeSmall`
        // for the sun, else the planet glyph tinted `p.iconColor`).
        let mut button = Button::new_alloc();
        button.set_name(&format!("tab-{tab_name}"));
        button.set_theme_type_variation("clearNoneTogglei");
        button.set_tooltip_text(&GString::from(label.as_str()));
        button.set_toggle_mode(true);
        button.set_button_group(&group);
        button.set_pressed_no_signal(tab_name == tab);
        button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
        database_apply_clear_none_toggle(&mut button);
        if kit::glyph(&icon_name).is_empty() {
            button.set_text(&GString::from(label.as_str()));
        } else {
            let mut glyph = factory::glyph_label(&icon_name, 32, color);
            glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
            glyph.set_mouse_filter(MouseFilter::IGNORE);
            button.add_child(&glyph);
        }
        {
            let target = dialog.clone();
            let state_for_tab = state.clone();
            let name_for_tab = tab_name.clone();
            let callable = Callable::from_fn("database_select_tab", move |_args: &[&Variant]| {
                if !target.is_instance_valid() {
                    return;
                }
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                database_select_tab(&mut bound, &state_for_tab, &name_for_tab);
            });
            button.connect("pressed", &callable);
        }
        tabs_node.add_child(&button);
    }
}

/// `Styles.clearNoneTogglei` (`Styles.java:351-356`): `up = none`,
/// `over = flatOver`, `down`/`checked` = `flatDown` (black centre in a 4 px
/// `Pal.accent` frame).
fn database_apply_clear_none_toggle(button: &mut Gd<Button>) {
    let mut over = StyleBoxFlat::new_gd();
    over.set_bg_color(styles::FLAT_OVER);
    let mut checked = StyleBoxFlat::new_gd();
    checked.set_bg_color(Color::BLACK);
    checked.set_border_color(styles::ACCENT);
    checked.set_border_width_all(4);
    button.add_theme_stylebox_override("normal", &StyleBoxEmpty::new_gd());
    button.add_theme_stylebox_override("hover", &over);
    button.add_theme_stylebox_override("pressed", &checked);
    button.add_theme_stylebox_override("hover_pressed", &checked);
    button.add_theme_stylebox_override("focus", &StyleBoxEmpty::new_gd());
}

/// `_select_tab`.
fn database_select_tab(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<DatabaseState>>,
    tab_name: &str,
) {
    state.borrow_mut().tab = tab_name.to_string();
    database_build_tabs(&dialog.to_gd(), state);
    database_rebuild(dialog, state);
}

/// `_rebuild`: the filtered `databaseCategory`/`databaseTag` grid.
fn database_rebuild(dialog: &mut MindDialog, state: &Rc<RefCell<DatabaseState>>) {
    let (mut grid, view, tab, query) = {
        let st = state.borrow();
        let Some(grid) = st.grid.clone() else {
            return;
        };
        let query = st
            .search
            .as_ref()
            .map(|field| field.get_text().to_string().to_lowercase())
            .unwrap_or_default();
        (grid, st.view.clone(), st.tab.clone(), query)
    };
    if !grid.is_instance_valid() {
        return;
    }
    for child in grid.get_children().iter_shared() {
        let mut child = child.clone();
        grid.remove_child(&child);
        child.queue_free();
    }
    // `cols = clamp((graphics.getWidth() - Scl.scl(30)) / Scl.scl(32 + 12), 1, 22)`.
    let columns = (((viewport_width(dialog) - 30.0) / 44.0) as i32).clamp(1, 22);
    // Port-only tables (GAPS #68 UI half): they replace the content grid.
    if tab == DATABASE_ACHIEVEMENTS_TAB || tab == DATABASE_STATISTICS_TAB {
        database_rebuild_tables(dialog, &mut grid, &tab, &query);
        return;
    }
    let mut has_result = false;
    for category_value in dict_array(&view, "categories").iter_shared() {
        let Ok(category) = category_value.try_to::<VarDictionary>() else {
            continue;
        };
        let rows = database_tag_rows(&category, &query, &tab);
        if rows.is_empty() {
            continue;
        }
        has_result = true;
        // `all.add("@database-category." + name).growX().left().color(Pal.accent)`.
        let category_label =
            factory::label(&kit::t(&format!("@{}", dict_text(&category, "label"))).to_string());
        {
            let mut control: Gd<Control> = category_label.clone().upcast();
            control.set_modulate(styles::ACCENT);
        }
        grid.add_child(&category_label);
        // `all.image().pad(5).padLeft(0).padRight(0).height(3).color(Pal.accent).growX()`.
        grid.add_child(&database_padded(
            database_divider(styles::ACCENT),
            0,
            5,
            DATABASE_ARC_BAR_SQUEEZE,
            5,
        ));
        // `all.table(sub -> ...).growX().left().padBottom(10)`.
        // code-instantiated: one section per category with live tag rows.
        let mut sub = VBoxContainer::new_alloc();
        sub.add_theme_constant_override("separation", 0);
        for row in rows {
            let tag_name = dict_text(&row, "name");
            if tag_name != "default" {
                // code-instantiated: tag header rows follow the live tag list.
                let mut header = HBoxContainer::new_alloc();
                header.add_theme_constant_override("separation", 0);
                let tag_label =
                    factory::label(&kit::t(&format!("@{}", dict_text(&row, "label"))).to_string());
                {
                    let mut control: Gd<Control> = tag_label.clone().upcast();
                    control.set_modulate(DATABASE_PAL_GRAY);
                }
                {
                    let mut rich_tag: Gd<RichTextLabel> = tag_label.clone().upcast();
                    rich_tag.set_autowrap_mode(AutowrapMode::OFF);
                    rich_tag.set_fit_content(true);
                }
                header.add_child(&tag_label);
                let mut line = database_padded(database_divider(DATABASE_PAL_GRAY), 5, 5, 5, 5);
                line.set_h_size_flags(SizeFlags::EXPAND_FILL);
                line.set_v_size_flags(SizeFlags::SHRINK_CENTER);
                header.add_child(&line);
                sub.add_child(&database_padded(header, 8, 4, 8, 4));
            }
            // `sub.table(list -> ...)`: `cols` cells of `iconMed` padded 3, the
            // row topped up with clear cells.
            // code-instantiated: one grid per `databaseTag` section.
            let mut list = GridContainer::new_alloc();
            list.set_name(&format!(
                "grid-{}-{}",
                dict_text(&category, "name"),
                tag_name
            ));
            list.set_columns(columns);
            list.add_theme_constant_override("h_separation", 6);
            list.add_theme_constant_override("v_separation", 6);
            let entries = dict_array(&row, "entries");
            for entry in entries.iter_shared() {
                list.add_child(&database_entry_button(&entry));
            }
            let fillers = (columns - entries.len() as i32).max(0);
            for _ in 0..fillers {
                // code-instantiated: clear filler cells keep the grid `cols` wide.
                let mut filler = Control::new_alloc();
                filler.set_custom_minimum_size(Vector2::new(DATABASE_ICON_MED, DATABASE_ICON_MED));
                filler.set_mouse_filter(MouseFilter::IGNORE);
                list.add_child(&filler);
            }
            sub.add_child(&database_padded(list, 3, 3, 3, 3));
        }
        grid.add_child(&database_padded(sub, 0, 0, 0, 10));
    }
    if !has_result {
        grid.add_child(&factory::label(&kit::t("@none.found").to_string()));
    }
}

/// The achievements/statistics table body (port-only Database tabs).
///
/// Achievements render `icon name progress` rows (unlocked `accent`, locked
/// `Pal.gray`; the progress column is `3/10` or `unlocked`), statistics render
/// `name value` rows. Both honour the search query and the accent rule the
/// content categories use (`DatabaseDialog.rebuild`'s header treatment).
fn database_rebuild_tables(
    dialog: &MindDialog,
    grid: &mut Gd<VBoxContainer>,
    tab: &str,
    query: &str,
) {
    let (header, mut lines, completion) = if tab == DATABASE_ACHIEVEMENTS_TAB {
        let rows = database_achievement_rows(dialog);
        let completion = kit::rows::completion(&rows);
        let lines: Vec<TableLine> = rows
            .into_iter()
            .filter(|row| query.is_empty() || row.name.to_lowercase().contains(query))
            .map(|row| TableLine {
                icon: if row.unlocked { "ok" } else { "cancel" },
                color: if row.unlocked {
                    styles::ACCENT
                } else {
                    DATABASE_PAL_GRAY
                },
                name: row.name.clone(),
                value: kit::rows::progress_text(&row),
                accent: kit::rows::row_color(&row) == "accent",
            })
            .collect();
        (DATABASE_ACHIEVEMENTS_TAB, lines, Some(completion))
    } else {
        let rows = database_stat_rows(dialog);
        let lines: Vec<TableLine> = rows
            .into_iter()
            .filter(|row| query.is_empty() || row.name.to_lowercase().contains(query))
            .map(|row| TableLine {
                icon: "",
                color: Color::WHITE,
                name: row.name.clone(),
                value: kit::rows::stat_value_text(&row),
                accent: false,
            })
            .collect();
        (DATABASE_STATISTICS_TAB, lines, None)
    };
    if lines.is_empty() {
        grid.add_child(&factory::label(&kit::t("@none.found").to_string()));
        return;
    }
    // Category header + the 3 px accent rule (`all.image().height(3)`).
    let mut title = header.to_string();
    if let Some((unlocked, total)) = completion {
        title = format!("{header}  {unlocked}/{total}");
    }
    let label = factory::label(&title);
    {
        let mut control: Gd<Control> = label.clone().upcast();
        control.set_modulate(styles::ACCENT);
    }
    grid.add_child(&label);
    grid.add_child(&database_padded(
        database_divider(styles::ACCENT),
        0,
        5,
        DATABASE_ARC_BAR_SQUEEZE,
        5,
    ));
    // code-instantiated: one row per table entry, rebuilt on every refresh.
    let mut list = VBoxContainer::new_alloc();
    list.add_theme_constant_override("separation", 4);
    for line in lines.drain(..) {
        // code-instantiated: `icon name ... value` row.
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 8);
        if !line.icon.is_empty() {
            let mut glyph = factory::glyph_label(line.icon, 24, line.color);
            glyph.set_v_size_flags(SizeFlags::SHRINK_CENTER);
            row.add_child(&glyph);
        }
        let name = factory::label(&line.name);
        {
            let mut control: Gd<Control> = name.clone().upcast();
            control.set_modulate(line.color);
            control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        }
        row.add_child(&name);
        let value_label = factory::label(&line.value);
        {
            let mut control: Gd<Control> = value_label.clone().upcast();
            control.set_modulate(if line.accent {
                styles::ACCENT
            } else {
                DATABASE_PAL_GRAY
            });
        }
        {
            let mut rich: Gd<RichTextLabel> = value_label.clone().upcast();
            rich.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        }
        row.add_child(&value_label);
        list.add_child(&row);
    }
    grid.add_child(&database_padded(list, 8, 4, 8, 12));
}

/// One rendered row of a port-only table.
struct TableLine {
    icon: &'static str,
    color: Color,
    name: String,
    value: String,
    accent: bool,
}

/// Non-empty tag rows for the active tab and search query.
fn database_tag_rows(category: &VarDictionary, query: &str, tab: &str) -> Vec<VarDictionary> {
    let mut rows = Vec::new();
    for tag_value in dict_array(category, "tags").iter_shared() {
        let Ok(tag) = tag_value.try_to::<VarDictionary>() else {
            continue;
        };
        let mut entries = VarArray::new();
        for entry_value in dict_array(&tag, "entries").iter_shared() {
            let Ok(entry) = entry_value.try_to::<VarDictionary>() else {
                continue;
            };
            if !database_entry_in_tab(&entry, tab) {
                continue;
            }
            if !query.is_empty() && !database_entry_matches(&entry, query) {
                continue;
            }
            entries.push(&entry.to_variant());
        }
        if entries.is_empty() {
            continue;
        }
        let mut row = VarDictionary::new();
        row.set("name", &GString::from(dict_text(&tag, "name").as_str()));
        row.set("label", &GString::from(dict_text(&tag, "label").as_str()));
        row.set("entries", &entries);
        rows.push(row);
    }
    rows
}

/// `tab == Planets.sun || allDatabaseTabs || databaseTabs.contains(tab)`.
fn database_entry_in_tab(entry: &VarDictionary, tab: &str) -> bool {
    if tab == DATABASE_ALL_TAB || dict_bool(entry, "all_tabs") {
        return true;
    }
    dict_array(entry, "tabs").iter_shared().any(|value| {
        value
            .try_to::<GString>()
            .map(|text| text == tab)
            .unwrap_or(false)
    })
}

/// `_entry_matches`.
fn database_entry_matches(entry: &VarDictionary, query: &str) -> bool {
    if dict_text(entry, "name").to_lowercase().contains(query) {
        return true;
    }
    database_entry_label(entry).to_lowercase().contains(query)
}

/// One grid cell: the content icon when unlocked (`ui.content.show`), else the
/// gray lock glyph (`new Image(Icon.lock, Pal.gray)`).
fn database_entry_button(entry_value: &Variant) -> Gd<Button> {
    let entry = entry_value.try_to::<VarDictionary>().unwrap_or_default();
    let entry_name = dict_text(&entry, "name");
    let unlocked = dict_bool(&entry, "unlocked");
    let mut button = if unlocked {
        let type_name = dict_text(&entry, "content_type");
        let regions = [
            format!("{type_name}-{entry_name}-ui"),
            format!("{type_name}-{entry_name}-full"),
            entry_name.clone(),
        ];
        let region_refs: Vec<&str> = regions.iter().map(|region| region.as_str()).collect();
        // code-instantiated: content icons are data-driven (atlas fallback chain).
        let mut image_button = factory::image_button_first(&region_refs, "selecti");
        image_button.set_expand_icon(true);
        image_button.add_theme_constant_override("icon_max_width", DATABASE_ICON_MED as i32);
        // `new Image(uiIcon, Color.lightGray)` lerping to white while hovered.
        image_button.add_theme_color_override("icon_normal_color", styles::ARC_LIGHT_GRAY);
        image_button.add_theme_color_override("icon_hover_color", Color::WHITE);
        image_button.add_theme_color_override("icon_pressed_color", Color::WHITE);
        image_button.add_theme_color_override("icon_hover_pressed_color", Color::WHITE);
        image_button.add_theme_color_override("icon_focus_color", styles::ARC_LIGHT_GRAY);
        for state in ["normal", "hover", "pressed", "hover_pressed", "focus"] {
            image_button.add_theme_stylebox_override(state, &StyleBoxEmpty::new_gd());
        }
        if image_button.get_button_icon().is_none() {
            image_button.set_text(&GString::from(database_entry_label(&entry).as_str()));
        }
        {
            let name_for_info = entry_name.clone();
            let callable =
                Callable::from_fn("database_content_info", move |_args: &[&Variant]| {
                    open_content_info(&name_for_info);
                });
            image_button.connect("pressed", &callable);
        }
        image_button
    } else {
        // code-instantiated: locked cells carry the gray lock glyph.
        let mut locked = Button::new_alloc();
        locked.set_theme_type_variation("selecti");
        let mut lock =
            factory::glyph_label("lock", (DATABASE_ICON_MED * 0.75) as i32, DATABASE_PAL_GRAY);
        lock.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        lock.set_mouse_filter(MouseFilter::IGNORE);
        locked.add_child(&lock);
        locked
    };
    if dict_bool(&entry, "banned") {
        // `new Image(Icon.cancel, Tmp.c1.set(Color.scarlet).a(0.6f))` over the
        // content icon when in a game with the block banned.
        let mut cross = factory::glyph_label("cancel", DATABASE_ICON_MED as i32, DATABASE_SCARLET);
        cross.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        cross.set_mouse_filter(MouseFilter::IGNORE);
        button.add_child(&cross);
    }
    button.set_name(&format!("content-{entry_name}"));
    // `.size(iconMed).pad(3)` (the pad is the grid separation).
    button.set_custom_minimum_size(Vector2::new(DATABASE_ICON_MED, DATABASE_ICON_MED));
    button.set_meta("unlocked", &unlocked.to_variant());
    if unlocked {
        button.set_tooltip_text(&GString::from(database_entry_label(&entry).as_str()));
    }
    button
}

/// Wraps `control` in a margin box (Arc cell pads: left, top, right, bottom).
fn database_padded<C: Inherits<Control>>(
    control: Gd<C>,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
) -> Gd<MarginContainer> {
    // code-instantiated: cell padding for a runtime section row.
    let mut box_ = MarginContainer::new_alloc();
    box_.add_theme_constant_override("margin_left", left);
    box_.add_theme_constant_override("margin_top", top);
    box_.add_theme_constant_override("margin_right", right);
    box_.add_theme_constant_override("margin_bottom", bottom);
    box_.set_mouse_filter(MouseFilter::IGNORE);
    let child: Gd<Control> = control.upcast();
    box_.add_child(&child);
    box_
}

/// Section divider (`all.image().height(3)` with the given color).
fn database_divider(color: Color) -> Gd<ColorRect> {
    // code-instantiated: one divider per runtime section row.
    let mut line = ColorRect::new_alloc();
    line.set_name("database-rule");
    line.set_color(color);
    line.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    line.set_h_size_flags(SizeFlags::EXPAND_FILL);
    line
}

/// Tab label: `@all` for the sun tab, else the localized planet name.
fn database_tab_label(tab: &VarDictionary, is_all: bool) -> String {
    if is_all {
        return kit::t("@all").to_string();
    }
    bundle_or(&dict_text(tab, "localized"), &dict_text(tab, "name"))
}

/// Entry label: bundle key when the model sent one (`<type>.<name>.name`).
fn database_entry_label(entry: &VarDictionary) -> String {
    bundle_or(&dict_text(entry, "localized"), &dict_text(entry, "name"))
}

/// `rrggbb` planet icon color -> Godot color (white when absent/malformed).
fn database_hex_color(value: &str) -> Color {
    if value.len() != 6 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Color::WHITE;
    }
    let channel = |range: std::ops::Range<usize>| -> u8 {
        u8::from_str_radix(&value[range], 16).unwrap_or(0)
    };
    Color::from_rgba8(channel(0..2), channel(2..4), channel(4..6), 0xff)
}
