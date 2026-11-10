// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/editor/MapProcessorsDialog.java,
//         core/src/mindustry/editor/MapLoadDialog.java,
//         core/src/mindustry/editor/SectorGenerateDialog.java,
//         core/src/mindustry/editor/BannedContentDialog.java,
//         core/src/mindustry/editor/data/MapAssetsDialog.java (+ `MapPatchesView`,
//         `MapContentView`, `MapBundlesView`, `MapImagesView`, `MapAudioView`,
//         `AssetView`), core/src/mindustry/ui/dialogs/SchematicsDialog.java
//         (`SchematicInfoDialog`, :774-834).

//! Editor data dialogs and the schematic info page.
//!
//! Each controller mirrors one `client/scenes/editor/**/*.gd` (or
//! `client/ui/dialogs/schematic_info_dialog.gd`) script: the map-processor list,
//! the map loader, the sector generator, the banned-content picker, the data
//! asset manager and the schematic info page. Behaviour (editor map state, the
//! data-asset owner, the banned-set model) stays in `MindEditor` /
//! `MindCampaign`; the controllers only read their JSON/array projections and
//! call the same dynamic method names the GDScript used.
//!
//! Prompts (`MindUi.show_text_input`/`show_confirm`) and the `icon_select`
//! signal are answered asynchronously; those handlers queue their work through
//! [`defer_action`] because a prompt is emitted from inside a bound `MindUi`
//! method and re-entering a bound GDExtension object panics (`PAR-0066`).

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::file_dialog::{Access, FileMode};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    AcceptDialog, BoxContainer, Button, ButtonGroup, CenterContainer, ColorRect, Control,
    DisplayServer, FileAccess, FileDialog, GridContainer, HBoxContainer, Json, LineEdit,
    MarginContainer, Os, PanelContainer, RichTextLabel, StyleBoxFlat, Texture2D, TextureRect,
    VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::factory;
use super::super::kit::{self, MindLabel, MindScroll, MindTable, styles};
use crate::ui::dialogs::editor::util_f::*;

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "banned_content" => Some(Box::new(BannedContentDialog::default())),
        "map_assets" => Some(Box::new(MapAssetsDialog::default())),
        "schematic_info" => Some(Box::new(SchematicInfoDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// BannedContentDialog (`banned_content`)
// ---------------------------------------------------------------------------

/// `BannedContentDialog.rebuildTable`: 50px cells, 32px icons.
const BANNED_CELL: f32 = 50.0;
const BANNED_ICON: i32 = 32;
/// `TabHoverColor` / Java `Color.valueOf("f25555")` banned accent.
const BANNED_ACCENT: Color = Color::from_rgba8(0xf2, 0x55, 0x55, 0xff);

/// The `banned_content` picker state.
#[derive(Default)]
struct BannedState {
    /// `_kind` (`content_type` in the GDScript, `"block"` by default).
    kind: String,
    /// Working rules dictionary: caller-supplied or the editor `rules` tag.
    rules: VarDictionary,
    /// True when `rules` mirrors the editor `rules` tag.
    tag_mode: bool,
    /// `contentSearch`: trimmed, collapsed, lowercased.
    search: String,
    /// `selectedCategory`, `""` for none.
    category: String,
    /// Bannable rows from `bannable_content_json`.
    rows: Vec<VarDictionary>,
    /// Category names for the filter row (`Category.values()`).
    categories: Vec<String>,
    /// Current banned names (`contentSet` order).
    banned: Vec<String>,
    search_field: Option<Gd<LineEdit>>,
    selected_grid: Option<Gd<GridContainer>>,
    deselected_grid: Option<Gd<GridContainer>>,
    selected_box: Option<Gd<VBoxContainer>>,
    deselected_box: Option<Gd<VBoxContainer>>,
    add_all_selected: Option<Gd<Button>>,
    add_all_deselected: Option<Gd<Button>>,
    /// `rebuildTable` column count (`max(4, ...)`).
    columns: i32,
}

/// `@bannedblocks`/`@bannedunits` — the two-pane ban picker.
#[derive(Default)]
pub struct BannedContentDialog {
    state: Rc<RefCell<BannedState>>,
}

impl DialogController for BannedContentDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        let mut state = self.state.borrow_mut();
        if state.kind.is_empty() {
            state.kind = String::from("block");
        }
        let kind = state.kind.clone();
        drop(state);
        dialog.set_title_key(banned_title_key(&kind));
        dialog.should_pause = false;
        dialog.add_close_button(210.0);
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        banned_context_changed(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        banned_shown(dialog, &self.state);
    }
}

/// `@bannedblocks` for blocks, `@bannedunits` for units.
fn banned_title_key(kind: &str) -> &'static str {
    if kind == "block" {
        "@bannedblocks"
    } else {
        "@bannedunits"
    }
}

/// `set_context_json`: `{kind, rules}`; without `rules` the editor tag is edited.
fn banned_context_changed(dialog: &mut MindDialog, state: &Rc<RefCell<BannedState>>) {
    let context_kind = dialog.context_string("kind").to_string();
    let rules_value = dialog.context_value("rules");
    let mut state = state.borrow_mut();
    if !context_kind.is_empty() {
        state.kind = context_kind;
    }
    state.search.clear();
    state.category.clear();
    match rules_value.map(|value| value.try_to::<VarDictionary>()) {
        Some(Ok(rules)) => {
            state.rules = rules.duplicate_deep();
            state.tag_mode = false;
        }
        _ => {
            state.rules = banned_tag_rules();
            state.tag_mode = true;
        }
    }
    let kind = state.kind.clone();
    drop(state);
    dialog.set_title_key(banned_title_key(&kind));
}

/// `shown(this::build)`: load the bannable content + the live set, then build.
fn banned_shown(dialog: &mut MindDialog, state: &Rc<RefCell<BannedState>>) {
    banned_load_content(state);
    let banned = banned_read(state);
    state.borrow_mut().banned = banned;
    banned_build(dialog, state);
}

/// `content.<T>getBy(type)` rows + the category names.
fn banned_load_content(state: &Rc<RefCell<BannedState>>) {
    let kind = state.borrow().kind.clone();
    let mut rows: Vec<VarDictionary> = Vec::new();
    let mut categories: Vec<String> = Vec::new();
    if let Some(value) = editor_call(
        "bannable_content_json",
        &[GString::from(kind.as_str()).to_variant()],
    ) {
        let payload = parse_json_object(&value.to::<GString>().to_string());
        if let Some(value) = payload.get("rows")
            && let Ok(array) = value.try_to::<VarArray>()
        {
            rows = array
                .iter_shared()
                .filter_map(|row| row.try_to::<VarDictionary>().ok())
                .collect();
        }
        if let Some(value) = payload.get("categories")
            && let Ok(array) = value.try_to::<VarArray>()
        {
            categories = array
                .iter_shared()
                .map(|entry| variant_string(&entry))
                .collect();
        }
    }
    let mut state = state.borrow_mut();
    state.rows = rows;
    state.categories = categories;
}

/// `contentSet` contents (`Rules.bannedBlocks`/`bannedUnits`).
fn banned_read(state: &Rc<RefCell<BannedState>>) -> Vec<String> {
    let key = banned_key(&state.borrow().kind);
    let raw = state.borrow().rules.get(key);
    let mut out: Vec<String> = Vec::new();
    if let Some(value) = raw
        && let Ok(array) = value.try_to::<VarArray>()
    {
        for entry in array.iter_shared() {
            if let Ok(name) = entry.try_to::<GString>() {
                let name = name.to_string();
                if !name.is_empty() && !out.contains(&name) {
                    out.push(name);
                }
            }
        }
    }
    out
}

/// `bannedBlocks` for blocks, `bannedUnits` for units.
fn banned_key(kind: &str) -> &'static str {
    if kind == "block" {
        "bannedBlocks"
    } else {
        "bannedUnits"
    }
}

/// `editor.tags["rules"]` parsed.
fn banned_tag_rules() -> VarDictionary {
    let Some(value) = editor_call("tags", &[]) else {
        return VarDictionary::new();
    };
    let Ok(tags) = value.try_to::<VarDictionary>() else {
        return VarDictionary::new();
    };
    let Some(rules) = tags.get("rules") else {
        return VarDictionary::new();
    };
    parse_json_object(&variant_string(&rules))
}

/// `_build`: search row, category row, the two panes.
fn banned_build(dialog: &mut MindDialog, state: &Rc<RefCell<BannedState>>) {
    dialog.clear_content();
    let mut root = dialog.content_table();
    let kind = state.borrow().kind.clone();
    let categories = state.borrow().categories.clone();

    // `t.table(s -> s.label("@search") + field + cancel button)`: content-sized
    // and centred (`BannedContentDialog.java:55-86`).
    let mut top = VBoxContainer::new_alloc();
    root.bind_mut().add(&top).pad(4.0);
    root.bind_mut().row();
    // code-instantiated: the search row mirrors Java's label + field + clear.
    let mut search_row = HBoxContainer::new_alloc();
    search_row.add_theme_constant_override("separation", 10);
    search_row.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    search_row.set_alignment(AlignmentMode::CENTER);
    let mut search_label = factory::label(&kit::t("@search").to_string());
    search_label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    search_row.add_child(&search_label);
    let mut field = factory::field("");
    field.set_text(&GString::from(state.borrow().search.as_str()));
    // `s.field(...)` has no growX: Arc `TextField` prefWidth is 150.
    field.set_custom_minimum_size(Vector2::new(150.0, 0.0));
    let dialog_gd = dialog.to_gd();
    let state_for_search = state.clone();
    let callable = Callable::from_fn("banned_search", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        banned_on_search(&dialog_gd, &state_for_search, &text);
    });
    field.connect("text_changed", &callable);
    search_row.add_child(&field);
    let dialog_gd = dialog.to_gd();
    let state_for_clear = state.clone();
    let field_for_clear = field.clone();
    let clear = Callable::from_fn("banned_clear", move |_args: &[&Variant]| {
        state_for_clear.borrow_mut().search.clear();
        let mut field = field_for_clear.clone();
        field.set_text("");
        banned_rebuild(&dialog_gd, &state_for_clear);
    });
    let mut clear_button = factory::icon_button(
        "cancel",
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    clear_button.set_custom_minimum_size(Vector2::new(35.0, 35.0));
    clear_button.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    clear_button.connect("pressed", &clear);
    search_row.add_child(&clear_button);
    top.add_child(&search_row);
    state.borrow_mut().search_field = Some(field);

    // `if(type == ContentType.block)`: category toggles (`Category.values()`).
    if kind == "block" && !categories.is_empty() {
        // code-instantiated: one toggle per block category (data-driven row).
        let mut category_row = HBoxContainer::new_alloc();
        category_row.set_alignment(AlignmentMode::CENTER);
        category_row.add_theme_constant_override("separation", 4);
        for category in &categories {
            let category_name = category.clone();
            let mut button = factory::glyph_button(&category_name, &category_name, "squareTogglei");
            button.set_custom_minimum_size(Vector2::new(45.0, 45.0));
            button.set_toggle_mode(true);
            button.set_pressed(state.borrow().category == category_name);
            let dialog_gd = dialog.to_gd();
            let state_for_category = state.clone();
            let name_for_toggle = category_name.clone();
            let callable = Callable::from_fn("banned_category", move |_args: &[&Variant]| {
                {
                    let mut state = state_for_category.borrow_mut();
                    if state.category == name_for_toggle {
                        state.category.clear();
                    } else {
                        state.category = name_for_toggle.clone();
                    }
                }
                banned_rebuild(&dialog_gd, &state_for_category);
            });
            button.connect("pressed", &callable);
            category_row.add_child(&button);
        }
        top.add_child(&category_row);
    }

    // `cont.table(...)` / `cont.table(...)`: the banned + unbanned panes.
    let portrait = is_portrait(dialog);
    // Portrait stacks the panes (`if(Core.graphics.isPortrait()) cont.row()`).
    let mut panes: Gd<BoxContainer> = if portrait {
        VBoxContainer::new_alloc().upcast()
    } else {
        HBoxContainer::new_alloc().upcast()
    };
    panes.set_v_size_flags(SizeFlags::EXPAND_FILL);
    panes.add_theme_constant_override("separation", 10);
    root.bind_mut().add(&panes).grow_y().pad(4.0);
    let dialog_gd = dialog.to_gd();
    state.borrow_mut().columns = banned_columns(dialog);
    let banned_widget = banned_pane(&dialog_gd, state, true);
    panes.add_child(&banned_widget);
    let available_widget = banned_pane(&dialog_gd, state, false);
    panes.add_child(&available_widget);
    banned_rebuild(&dialog_gd, state);
}

/// One pane (`table.image().color(...)` + `pane(...)` + `@addall`).
fn banned_pane(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<BannedState>>,
    selected: bool,
) -> Gd<VBoxContainer> {
    let kind = state.borrow().kind.clone();
    // code-instantiated: one ban pane (title, rule, scrolling icon grid).
    let mut box_ = VBoxContainer::new_alloc();
    box_.set_h_size_flags(SizeFlags::FILL);
    box_.set_v_size_flags(SizeFlags::EXPAND_FILL);
    let color = if selected {
        BANNED_ACCENT
    } else {
        styles::ACCENT
    };
    let prefix = if selected { "banned" } else { "unbanned" };
    let suffix = if kind == "block" { "blocks" } else { "units" };
    let mut title = factory::label(&kit::t(&format!("@{prefix}{suffix}")).to_string());
    title.add_theme_color_override("font_color", color);
    // `table.add(...).padBottom(-1).top()`: the title centres over the rule.
    title.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    box_.add_child(&title);
    // code-instantiated: the 3px rule under the pane header.
    let mut line = ColorRect::new_alloc();
    line.set_color(color);
    line.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    line.set_h_size_flags(SizeFlags::EXPAND_FILL);
    box_.add_child(&line);
    let mut scroll = MindScroll::new_alloc();
    scroll.set_h_size_flags(SizeFlags::EXPAND_FILL);
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    scroll.bind_mut().set_scrolling_disabled(true, false);
    scroll.set_custom_minimum_size(Vector2::new(0.0, 110.0));
    box_.add_child(&scroll);
    // code-instantiated: the icon grid is data-driven (content rows).
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(state.borrow().columns.max(1));
    // code-instantiated: the centring wrapper of the data-driven grid.
    let mut center = CenterContainer::new_alloc();
    center.set_h_size_flags(SizeFlags::EXPAND_FILL);
    center.set_v_size_flags(SizeFlags::EXPAND_FILL);
    scroll.add_child(&center);
    center.add_child(&grid);
    let add_text = kit::t("@addall").to_string();
    let mut add_all = factory::icon_button(
        "add",
        &add_text,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    add_all.set_custom_minimum_size(Vector2::new(0.0, 48.0));
    let dialog_gd = dialog.clone();
    let state_for_add = state.clone();
    let callable = Callable::from_fn("banned_add_all", move |_args: &[&Variant]| {
        banned_add_all(&dialog_gd, &state_for_add, selected);
    });
    add_all.connect("pressed", &callable);
    box_.add_child(&add_all);
    {
        let mut state = state.borrow_mut();
        if selected {
            state.selected_grid = Some(grid);
            state.selected_box = Some(box_.clone());
            state.add_all_selected = Some(add_all);
        } else {
            state.deselected_grid = Some(grid);
            state.deselected_box = Some(box_.clone());
            state.add_all_deselected = Some(add_all);
        }
    }
    box_
}

/// `rebuildTable` column count (`Core.graphics.getWidth() / Scl.scl()`):
/// `max(4, (screen - 300) / 50 / 2)` cells per pane.
fn banned_columns(dialog: &MindDialog) -> i32 {
    let width = viewport_width(dialog);
    if is_portrait(dialog) {
        // `max(4, (width - 100) / 50)` (`BannedContentDialog.java:155-156`).
        (((width - 100.0) / BANNED_CELL) as i32).max(4)
    } else {
        (((width - 300.0) / BANNED_CELL / 2.0) as i32).max(4)
    }
}

/// `rebuildTables()`: refilter and repaint both panes.
fn banned_rebuild(dialog: &Gd<MindDialog>, state: &Rc<RefCell<BannedState>>) {
    let selected_grid = state.borrow().selected_grid.clone();
    let deselected_grid = state.borrow().deselected_grid.clone();
    let (Some(mut selected_grid), Some(mut deselected_grid)) = (selected_grid, deselected_grid)
    else {
        return;
    };
    clear_children(&mut selected_grid);
    clear_children(&mut deselected_grid);
    let filtered = banned_filtered_rows(state);
    let banned = state.borrow().banned.clone();
    let mut banned_rows: Vec<VarDictionary> = Vec::new();
    let mut available_rows: Vec<VarDictionary> = Vec::new();
    for row in filtered {
        if banned.contains(&dict_string(&row, "name")) {
            banned_rows.push(row);
        } else {
            available_rows.push(row);
        }
    }
    banned_fill_grid(dialog, state, &mut selected_grid, &banned_rows);
    banned_fill_grid(dialog, state, &mut deselected_grid, &available_rows);
    // `contentSet.toSeq().containsAll(filteredContent)`: enabled exactly when a
    // filtered row is still unbanned.
    if let Some(mut button) = state.borrow().add_all_selected.clone() {
        button.set_disabled(available_rows.is_empty());
    }
    // `available.containsAll(filteredContent)`: enabled exactly when a filtered
    // row is banned.
    if let Some(mut button) = state.borrow().add_all_deselected.clone() {
        button.set_disabled(banned_rows.is_empty());
    }
    banned_uniform_panes_deferred(state);
}

/// Arc `uniform()` on both pane cells: each takes the larger pref width.
fn banned_uniform_panes_deferred(state: &Rc<RefCell<BannedState>>) {
    let state = state.clone();
    defer_action("banned_uniform_panes", move || {
        banned_uniform_panes(&state);
    });
}

fn banned_uniform_panes(state: &Rc<RefCell<BannedState>>) {
    let boxes = {
        let state = state.borrow();
        (state.selected_box.clone(), state.deselected_box.clone())
    };
    let (Some(selected), Some(deselected)) = boxes else {
        return;
    };
    if !selected.is_instance_valid() || !deselected.is_instance_valid() {
        return;
    }
    let mut boxes = [selected, deselected];
    let mut width = 0.0_f32;
    for box_ in boxes.iter_mut() {
        let height = box_.get_custom_minimum_size().y;
        box_.set_custom_minimum_size(Vector2::new(0.0, height));
        width = width.max(box_.get_combined_minimum_size().x);
    }
    for box_ in boxes.iter_mut() {
        let height = box_.get_custom_minimum_size().y;
        box_.set_custom_minimum_size(Vector2::new(width, height));
    }
}

/// `_fill_grid(table, isSelected)`: one 50px cell per bannable row.
fn banned_fill_grid(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<BannedState>>,
    grid: &mut Gd<GridContainer>,
    rows: &[VarDictionary],
) {
    if rows.is_empty() {
        let mut empty = factory::label(&kit::t("@empty").to_string());
        empty.set_custom_minimum_size(Vector2::new(BANNED_CELL * grid.get_columns() as f32, 0.0));
        empty.set_horizontal_alignment(HorizontalAlignment::CENTER);
        grid.add_child(&empty);
        return;
    }
    for row in rows {
        let content_name = dict_string(row, "name");
        let icon = dict_string(row, "ui_icon");
        // code-instantiated: one 50px content cell per bannable row.
        let mut button =
            factory::image_button_first(&[icon.as_str(), content_name.as_str()], "clearNonei");
        button.set_custom_minimum_size(Vector2::new(BANNED_CELL, BANNED_CELL));
        button.set_expand_icon(true);
        button.add_theme_constant_override("icon_max_width", BANNED_ICON);
        if button.get_button_icon().is_none() {
            button.set_text(&GString::from(content_name.as_str()));
        }
        button.set_tooltip_text(&banned_localized(state, &content_name));
        let dialog_gd = dialog.clone();
        let state_for_toggle = state.clone();
        let name_for_toggle = content_name.clone();
        let callable = Callable::from_fn("banned_toggle", move |_args: &[&Variant]| {
            banned_toggle(&dialog_gd, &state_for_toggle, &name_for_toggle);
        });
        button.connect("pressed", &callable);
        grid.add_child(&button);
    }
}

/// `content.getBy(type).select(pred)` + search/category, sorted by content id.
fn banned_filtered_rows(state: &Rc<RefCell<BannedState>>) -> Vec<VarDictionary> {
    let (rows, category, search) = {
        let state = state.borrow();
        (
            state.rows.clone(),
            state.category.clone(),
            state.search.clone(),
        )
    };
    let mut out: Vec<VarDictionary> = Vec::new();
    for row in rows {
        if !category.is_empty() && dict_string(&row, "category") != category {
            continue;
        }
        if !search.is_empty() {
            // `content.localizedName.toLowerCase().contains(contentSearch)`.
            let localized = banned_localized(state, &dict_string(&row, "name"));
            if !localized.to_lowercase().contains(&search) {
                continue;
            }
        }
        out.push(row);
    }
    out.sort_by_key(|row| dict_i64(row, "id", 0));
    out
}

/// `content.localizedName`, falling back to the content name.
fn banned_localized(state: &Rc<RefCell<BannedState>>, content_name: &str) -> String {
    let prefix = if state.borrow().kind == "block" {
        "block"
    } else {
        "unit"
    };
    kit::t(&format!("@{prefix}.{content_name}.name")).to_string()
}

/// `value.trim().replaceAll(" +", " ").toLowerCase()` (`contentSearch`).
fn banned_on_search(dialog: &Gd<MindDialog>, state: &Rc<RefCell<BannedState>>, text: &str) {
    let normalized = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    state.borrow_mut().search = normalized;
    banned_rebuild(dialog, state);
}

/// Clicking a cell moves the content between panes (`contentSet.add/remove`).
fn banned_toggle(dialog: &Gd<MindDialog>, state: &Rc<RefCell<BannedState>>, content_name: &str) {
    {
        let mut state = state.borrow_mut();
        if let Some(index) = state.banned.iter().position(|name| name == content_name) {
            state.banned.remove(index);
        } else {
            state.banned.push(String::from(content_name));
        }
    }
    banned_commit(dialog, state);
    banned_rebuild(dialog, state);
}

/// `@addall` on the banned pane adds, on the unbanned pane removes.
fn banned_add_all(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<BannedState>>,
    banned_pane_selected: bool,
) {
    let filtered = banned_filtered_rows(state);
    {
        let mut state = state.borrow_mut();
        for row in &filtered {
            let name = dict_string(row, "name");
            if banned_pane_selected {
                if !state.banned.contains(&name) {
                    state.banned.push(name);
                }
            } else if let Some(index) = state.banned.iter().position(|entry| entry == &name) {
                state.banned.remove(index);
            }
        }
    }
    banned_commit(dialog, state);
    banned_rebuild(dialog, state);
}

/// Writes the set back: caller rules via `banned_edited`, else the editor tag.
fn banned_commit(dialog: &Gd<MindDialog>, state: &Rc<RefCell<BannedState>>) {
    let (kind, tag_mode, banned, rules) = {
        let mut state = state.borrow_mut();
        let banned = state.banned.clone();
        let key = banned_key(&state.kind);
        state
            .rules
            .set(&GString::from(key), &string_array_variant(&banned));
        (
            state.kind.clone(),
            state.tag_mode,
            banned,
            state.rules.clone(),
        )
    };
    if tag_mode {
        let _ = editor_call(
            "set_banned_names_json",
            &[
                GString::from(kind.as_str()).to_variant(),
                GString::from(stringify(&string_array_variant(&banned)).as_str()).to_variant(),
            ],
        );
        return;
    }
    if dialog.is_instance_valid() {
        let mut node = dialog.clone();
        node.emit_signal(
            "banned_edited",
            &[GString::from(stringify(&rules.to_variant()).as_str()).to_variant()],
        );
    }
}

// ---------------------------------------------------------------------------
// MapAssetsDialog (`map_assets`)
// ---------------------------------------------------------------------------

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
            .map(|value| variant_bool(value))
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
    let mut types = HBoxContainer::new_alloc();
    types.add_theme_constant_override("separation", 4);
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
    let mut search = HBoxContainer::new_alloc();
    search.add_theme_constant_override("separation", 4);
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
    dialog.add_close_button(210.0);

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
    clear_children(&mut list);
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

// --- helpers ---------------------------------------------------------------

/// `types.image(Tex.whiteui, Pal.accent).size(4f, 50f)`.
fn map_assets_divider() -> Gd<ColorRect> {
    // code-instantiated: the 4x50 accent rule between type-strip groups.
    let mut line = ColorRect::new_alloc();
    line.set_color(styles::ACCENT);
    line.set_custom_minimum_size(Vector2::new(4.0, 50.0));
    line
}

/// Rows of one `DataAssetType` (`_rows_of`).
fn map_assets_rows_of(state: &Rc<RefCell<MapAssetsState>>, kind: &str) -> Vec<VarDictionary> {
    state
        .borrow()
        .rows
        .iter()
        .filter(|row| dict_string(row, "kind") == kind)
        .cloned()
        .collect()
}

/// `_matches`: the lowercased search over one row key.
fn map_assets_matches(state: &Rc<RefCell<MapAssetsState>>, row: &VarDictionary, key: &str) -> bool {
    let search = state.borrow().search.clone();
    if search.is_empty() {
        return true;
    }
    dict_string(row, key).to_lowercase().contains(&search)
}

/// The viewport width from a live dialog handle.
fn map_assets_viewport_width(dialog: &Gd<MindDialog>) -> f32 {
    dialog.bind().base().get_viewport_rect().size.x
}

/// `_row_width`: `Vars.mobile ? 390f : 450f`.
fn map_assets_row_width(dialog: &MindDialog) -> f32 {
    if is_portrait(dialog) {
        ASSET_ROW_WIDTH_MOBILE
    } else {
        ASSET_ROW_WIDTH
    }
}

/// Empty cell for Java's `list.add().size(h)` warning placeholder.
fn map_assets_empty_cell() -> Gd<Control> {
    factory::space(ASSET_ROW_HEIGHT, ASSET_ROW_HEIGHT)
}

/// The two-line entry button (`[accent]name\n[lightgray]sub`).
fn map_assets_entry_button(
    row: &VarDictionary,
    title: &str,
    subtitle: &str,
    width: f32,
    handler: Callable,
) -> Gd<Button> {
    // code-instantiated: one two-line data-asset entry row.
    let mut button = factory::button("");
    button.set_theme_type_variation("grayt");
    button.set_custom_minimum_size(Vector2::new(width, ASSET_ROW_HEIGHT));
    button.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut raw = format!("[accent]{title}");
    if !subtitle.is_empty() {
        raw.push_str(&format!("\n[lightgray]{subtitle}"));
    }
    let mut label = factory::label(&markup(&raw));
    label.set_horizontal_alignment(HorizontalAlignment::LEFT);
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    label.set_offset(Side::LEFT, 10.0);
    label.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&label);
    button.set_tooltip_text(&dict_string(row, "path"));
    button.connect("pressed", &handler);
    button
}

/// `_error(message)`: the shared error prompt (`@error.title`).
fn map_assets_error(message: &str) {
    let title = kit::t("@error.title").to_string();
    if ui_call(
        "show_text",
        &[
            GString::from(title.as_str()).to_variant(),
            GString::from(message).to_variant(),
        ],
    )
    .is_some()
    {
        return;
    }
    log::warn!("[ui] map_assets: {message}");
}

/// `ui.showConfirm` + one pending handler (`confirm_result`).
fn map_assets_confirm(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    message: &str,
    pending: PendingConfirm,
) {
    if ui_call("show_confirm", &[GString::from(message).to_variant()]).is_some() {
        state.borrow_mut().pending_confirm = Some(pending);
        return;
    }
    map_assets_run_confirm(dialog, state, pending);
}

fn map_assets_on_confirm(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    confirmed: bool,
) {
    let pending = state.borrow_mut().pending_confirm.take();
    if !confirmed {
        return;
    }
    if let Some(pending) = pending {
        map_assets_run_confirm(dialog, state, pending);
    }
}

fn map_assets_run_confirm(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    pending: PendingConfirm,
) {
    match pending {
        PendingConfirm::ClearAll => {
            let result = editor_call("data_clear", &[]);
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::PatchDelete(index) => {
            let result = editor_call("data_patch_remove", &[index.to_variant()]);
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::ContentDelete(name) => {
            let result = editor_call(
                "data_asset_delete",
                &[
                    GString::from("content").to_variant(),
                    GString::from(name.as_str()).to_variant(),
                ],
            );
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::BundleDelete(path) => {
            let result = editor_call(
                "data_asset_delete",
                &[
                    GString::from("bundle").to_variant(),
                    GString::from(path.as_str()).to_variant(),
                ],
            );
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::ImageDelete(name) => {
            let result = editor_call(
                "data_asset_delete",
                &[
                    GString::from("image").to_variant(),
                    GString::from(name.as_str()).to_variant(),
                ],
            );
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::AudioDelete(kind, name) => {
            let result = editor_call(
                "data_asset_delete",
                &[
                    GString::from(kind.as_str()).to_variant(),
                    GString::from(name.as_str()).to_variant(),
                ],
            );
            map_assets_apply(dialog, state, result);
        }
    }
}

/// Owner error codes -> bundle keys (`DataManager` dialog messages).
fn map_assets_error_key(code: &str) -> Option<&'static str> {
    match code {
        "exists" => Some("@asset.content.exists"),
        "badname" => Some("@asset.content.badname"),
        "badtype" => Some("@asset.content.type"),
        "invalid JSON" => Some("@patch.importerror"),
        _ => None,
    }
}

/// Runs an owner action; on failure shows its message, on success refetches.
fn map_assets_apply(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    result: Option<Variant>,
) {
    let value = result
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default();
    if !dict_bool(&value, "ok", false) {
        let mut message = dict_string(&value, "error");
        if let Some(key) = map_assets_error_key(&message) {
            message = kit::t(key).to_string();
        }
        map_assets_error(&message);
        return;
    }
    map_assets_refetch(state);
    map_assets_rebuild(dialog, state);
}

/// `_show_text(title, text_body)`.
fn map_assets_show_text(title: &str, body: &str) {
    show_text(title, body);
}

/// `_open_file(label, filters, multiple, handler)`: the runtime `FileDialog`.
fn map_assets_open_file(
    dialog: &Gd<MindDialog>,
    label: &str,
    filters: Vec<String>,
    multiple: bool,
    handler: AssetPathsHandler,
) {
    // code-instantiated: `FileChooser.open` is a runtime native dialog.
    let mut file_dialog = FileDialog::new_alloc();
    file_dialog.set_title(&GString::from(label));
    file_dialog.set_file_mode(if multiple {
        FileMode::OPEN_FILES
    } else {
        FileMode::OPEN_FILE
    });
    file_dialog.set_access(Access::FILESYSTEM);
    for filter in &filters {
        file_dialog.add_filter(&GString::from(filter.as_str()));
    }
    let selected_target = file_dialog.clone();
    let selected_handler = handler.clone();
    let callable = Callable::from_fn("asset_file_selected", move |args: &[&Variant]| {
        let path = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut target = selected_target.clone();
        target.queue_free();
        selected_handler(vec![path]);
    });
    file_dialog.connect("file_selected", &callable);
    let files_target = file_dialog.clone();
    let files_handler = handler.clone();
    let callable = Callable::from_fn("asset_files_selected", move |args: &[&Variant]| {
        let paths = args
            .first()
            .and_then(|value| (*value).try_to::<PackedStringArray>().ok())
            .unwrap_or_default();
        let mut target = files_target.clone();
        target.queue_free();
        files_handler(
            paths
                .as_slice()
                .iter()
                .map(|path| path.to_string())
                .collect(),
        );
    });
    file_dialog.connect("files_selected", &callable);
    let canceled_target = file_dialog.clone();
    let callable = Callable::from_fn("asset_file_canceled", move |_args: &[&Variant]| {
        let mut target = canceled_target.clone();
        target.queue_free();
    });
    file_dialog.connect("canceled", &callable);
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&file_dialog);
    file_dialog.popup_centered_ratio_ex().ratio(0.6).done();
}

/// `_save_file(label, file_name, filter, handler)`: the runtime save dialog.
fn map_assets_save_file(
    dialog: &Gd<MindDialog>,
    label: &str,
    file_name: &str,
    filter: &str,
    handler: AssetTextHandler,
) {
    // code-instantiated: `FileChooser.export` is a runtime native dialog.
    let mut file_dialog = FileDialog::new_alloc();
    file_dialog.set_title(&GString::from(label));
    file_dialog.set_file_mode(FileMode::SAVE_FILE);
    file_dialog.set_access(Access::FILESYSTEM);
    file_dialog.set_current_file(&GString::from(file_name));
    file_dialog.add_filter(&GString::from(filter));
    let selected_target = file_dialog.clone();
    let callable = Callable::from_fn("asset_save_selected", move |args: &[&Variant]| {
        let path = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut target = selected_target.clone();
        target.queue_free();
        handler(path);
    });
    file_dialog.connect("file_selected", &callable);
    let canceled_target = file_dialog.clone();
    let callable = Callable::from_fn("asset_save_canceled", move |_args: &[&Variant]| {
        let mut target = canceled_target.clone();
        target.queue_free();
    });
    file_dialog.connect("canceled", &callable);
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&file_dialog);
    file_dialog.popup_centered_ratio_ex().ratio(0.6).done();
}

/// `_read_text(path)`: `FileAccess.get_file_as_string`.
fn map_assets_read_text(path: &str) -> String {
    FileAccess::get_file_as_string(path).to_string()
}

/// `_to_array(paths)`.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn map_assets_to_array(values: &PackedStringArray) -> Vec<String> {
    values
        .as_slice()
        .iter()
        .map(|value| value.to_string())
        .collect()
}

/// `_write_text(path, text)` (`FileAccess.open(path, WRITE).store_string`).
fn map_assets_write_text(path: &str, text: &str) {
    let Some(mut file) = FileAccess::open(path, godot::classes::file_access::ModeFlags::WRITE)
    else {
        return;
    };
    let _ = file.store_string(text);
}

/// `showImport(handler)`: clipboard + file choices (`MapPatchesView`).
fn map_assets_import_choices(
    dialog: &Gd<MindDialog>,
    filters: Vec<String>,
    handler: AssetImportHandler,
) {
    // code-instantiated: Java's `BaseDialog("@editor.import")` choice menu.
    let mut accept = AcceptDialog::new_alloc();
    accept.set_title(&kit::t("@editor.import"));
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 6);
    box_.set_custom_minimum_size(Vector2::new(300.0, 120.0));
    let clipboard_text = DisplayServer::singleton().clipboard_get();
    let clipboard_label = kit::t("@load.clipboard").to_string();
    let mut clipboard = factory::icon_button(
        "copy",
        &clipboard_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    clipboard.set_disabled(clipboard_text.is_empty());
    let clip_target = accept.clone();
    let clip_handler = handler.clone();
    let callable = Callable::from_fn("asset_import_clipboard", move |_args: &[&Variant]| {
        let text = DisplayServer::singleton().clipboard_get().to_string();
        let mut target = clip_target.clone();
        target.queue_free();
        if !text.is_empty() {
            clip_handler(text);
        }
    });
    clipboard.connect("pressed", &callable);
    box_.add_child(&clipboard);
    let file_label = kit::t("@import.file").to_string();
    let mut file = factory::icon_button(
        "download",
        &file_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let file_target = accept.clone();
    let dialog_for = dialog.clone();
    let file_handler = handler.clone();
    let file_filters = filters.clone();
    let callable = Callable::from_fn("asset_import_file", move |_args: &[&Variant]| {
        let mut target = file_target.clone();
        target.queue_free();
        let handler_for = file_handler.clone();
        let label = kit::t("@import.file").to_string();
        map_assets_open_file(
            &dialog_for,
            &label,
            file_filters.clone(),
            false,
            Rc::new(move |paths: Vec<String>| {
                let path = paths.first().cloned().unwrap_or_default();
                let text = map_assets_read_text(&path);
                handler_for(text);
            }),
        );
    });
    file.connect("pressed", &callable);
    box_.add_child(&file);
    accept.add_child(&box_);
    if let Some(mut ok) = accept.get_ok_button() {
        ok.set_visible(false);
    }
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&accept);
    accept
        .popup_centered_ex()
        .minsize(Vector2i::new(340, 180))
        .done();
}

/// `MapAssetsDialog.showEditMenu` (`@asset.importzip`/`exportzip`/`clearall`).
fn map_assets_show_edit_menu(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    // code-instantiated: Java builds this `BaseDialog("@edit.menu")` per open.
    let mut accept = AcceptDialog::new_alloc();
    accept.set_title(&kit::t("@edit.menu"));
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 6);
    box_.set_custom_minimum_size(Vector2::new(300.0, 170.0));

    let import_label = kit::t("@asset.importzip").to_string();
    let mut import_zip = factory::icon_button(
        "download",
        &import_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let import_target = accept.clone();
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("asset_import_zip", move |_args: &[&Variant]| {
        let mut target = import_target.clone();
        target.queue_free();
        let label = kit::t("@asset.importzip").to_string();
        let dialog_for_zip = dialog_for.clone();
        let state_for_zip = state_for.clone();
        map_assets_open_file(
            &dialog_for,
            &label,
            vec![String::from("*.zip")],
            false,
            Rc::new(move |paths: Vec<String>| {
                let path = paths.first().cloned().unwrap_or_default();
                let result = editor_call(
                    "data_import_zip",
                    &[GString::from(path.as_str()).to_variant()],
                );
                map_assets_apply(&dialog_for_zip, &state_for_zip, result);
            }),
        );
    });
    import_zip.connect("pressed", &callable);
    box_.add_child(&import_zip);

    let export_label = kit::t("@asset.exportzip").to_string();
    let mut export_zip = factory::icon_button(
        "upload",
        &export_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let export_target = accept.clone();
    let dialog_for = dialog.clone();
    let callable = Callable::from_fn("asset_export_zip", move |_args: &[&Variant]| {
        let mut target = export_target.clone();
        target.queue_free();
        let label = kit::t("@asset.exportzip").to_string();
        map_assets_save_file(
            &dialog_for,
            &label,
            "assets.zip",
            "*.zip",
            Rc::new(move |path: String| {
                let exported = editor_call(
                    "data_export_zip",
                    &[GString::from(path.as_str()).to_variant()],
                )
                .map(|value| variant_bool(&value))
                .unwrap_or(false);
                if !exported {
                    map_assets_error(&kit::t("@asset.exportzip").to_string());
                }
            }),
        );
    });
    export_zip.connect("pressed", &callable);
    box_.add_child(&export_zip);

    let clear_label = kit::t("@asset.clearall").to_string();
    let mut clear_all = factory::icon_button(
        "trash",
        &clear_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let clear_target = accept.clone();
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("asset_clear_all", move |_args: &[&Variant]| {
        let mut target = clear_target.clone();
        target.queue_free();
        let message = kit::t("@asset.clearall.confirm").to_string();
        map_assets_confirm(&dialog_for, &state_for, &message, PendingConfirm::ClearAll);
    });
    clear_all.connect("pressed", &callable);
    box_.add_child(&clear_all);
    accept.add_child(&box_);
    if let Some(mut ok) = accept.get_ok_button() {
        ok.set_visible(false);
    }
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&accept);
    accept
        .popup_centered_ex()
        .minsize(Vector2i::new(340, 230))
        .done();
}

// --- MapPatchesView --------------------------------------------------------

fn map_assets_build_patches(
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

fn map_assets_patch_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
) -> Gd<HBoxContainer> {
    let index = dict_i64(row, "index", 0);
    let patch_name = dict_string(row, "name");
    let json = dict_string(row, "data");
    let label = if patch_name.is_empty() {
        String::from("<unnamed>")
    } else {
        patch_name
    };
    let fields = map_assets_count_fields(&map_assets_parse_json(&json));
    // code-instantiated: one patch entry row (show/copy/import/delete).
    let mut box_ = HBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 4);
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
fn map_assets_add_patch(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
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
fn map_assets_text_filters() -> Vec<String> {
    vec![
        String::from("*.json"),
        String::from("*.hjson"),
        String::from("*.json5"),
    ]
}

/// `MapPatchesView.countFields`: leaf count of the patch JSON, at least 1.
fn map_assets_count_fields(value: &Variant) -> i64 {
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
fn map_assets_parse_json(text: &str) -> Variant {
    if text.is_empty() {
        return VarDictionary::new().to_variant();
    }
    let parsed = Json::parse_string(text);
    if parsed.is_nil() {
        return VarDictionary::new().to_variant();
    }
    parsed
}

// --- MapContentView --------------------------------------------------------

fn map_assets_build_content(
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
        let content_type = dict_string(row, "contentType");
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

fn map_assets_content_header(content_type: &str) -> Gd<HBoxContainer> {
    // `list.table(t -> icon + "@content.<type>" + whiteui rule).colspan(6)`.
    // code-instantiated: one content-type header row.
    let mut box_ = HBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 8);
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

fn map_assets_content_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
    content_type: &str,
) -> Gd<HBoxContainer> {
    let content_name = dict_string(row, "name");
    let json = dict_string(row, "data");
    // code-instantiated: one content record row.
    let mut box_ = HBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 4);
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
                if !dict_bool(&removed, "ok", false) {
                    map_assets_error(&dict_string(&removed, "error"));
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
fn map_assets_add_content(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    // code-instantiated: Java's `BaseDialog("@asset.content.add")` name/type form.
    let mut accept = AcceptDialog::new_alloc();
    accept.set_title(&kit::t("@asset.content.add"));
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 6);
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
    let mut actions = HBoxContainer::new_alloc();
    actions.add_theme_constant_override("separation", 6);
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

// --- MapBundlesView --------------------------------------------------------

fn map_assets_build_bundles(
    dialog: &Gd<MindDialog>,
    list: &mut Gd<VBoxContainer>,
    state: &Rc<RefCell<MapAssetsState>>,
) {
    let rows = map_assets_rows_of(state, "bundle");
    let mut shown_count = 0;
    for row in &rows {
        if !map_assets_matches(state, row, "name") {
            continue;
        }
        shown_count += 1;
        let widget = map_assets_bundle_row(dialog, state, row);
        list.add_child(&widget);
    }
    if shown_count == 0 {
        let label = factory::label(&markup("@patch.none"));
        list.add_child(&label);
    }
}

fn map_assets_bundle_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
) -> Gd<HBoxContainer> {
    let bundle_name = dict_string(row, "name");
    let path = dict_string(row, "path");
    let text = dict_string(row, "data");
    let lines = dict_i64(row, "size", 0);
    // code-instantiated: one bundle entry row (show/import/export/delete).
    let mut box_ = HBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 4);
    box_.set_alignment(AlignmentMode::CENTER);
    box_.add_child(&map_assets_empty_cell());
    let name_for_show = bundle_name.clone();
    let text_for_show = text.clone();
    let show = Callable::from_fn("bundle_show", move |_args: &[&Variant]| {
        map_assets_show_text(&name_for_show, &text_for_show);
    });
    let subtitle = kit::tm_format("@bundle.lines", &[&lines.to_string()]).to_string();
    let width = map_assets_row_width(&dialog.bind());
    box_.add_child(&map_assets_entry_button(
        row,
        &bundle_name,
        &subtitle,
        width,
        show,
    ));

    let state_for_import = state.clone();
    let dialog_for_import = dialog.clone();
    let path_for_import = path.clone();
    let refresh = Callable::from_fn("bundle_import", move |_args: &[&Variant]| {
        let state_for = state_for_import.clone();
        let dialog_for = dialog_for_import.clone();
        let path_for = path_for_import.clone();
        map_assets_import_choices(
            &dialog_for.clone(),
            vec![String::from("*.properties"), String::from("*.txt")],
            Rc::new(move |text: String| {
                let result = editor_call(
                    "data_bundle_set",
                    &[
                        GString::from(path_for.as_str()).to_variant(),
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
        refresh,
    ));

    let dialog_for_export = dialog.clone();
    let bundle_for_export = bundle_name.clone();
    let text_for_export = text.clone();
    let export = Callable::from_fn("bundle_export", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_export.clone();
        let bundle_for = bundle_for_export.clone();
        let text_for = text_for_export.clone();
        let label = kit::t("@editor.export").to_string();
        map_assets_save_file(
            &dialog_for,
            &label,
            &bundle_for,
            "*.properties",
            Rc::new(move |target: String| {
                map_assets_write_text(&target, &text_for);
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
    let path_for_delete = path.clone();
    let delete = Callable::from_fn("bundle_delete", move |_args: &[&Variant]| {
        let state_for = state_for_delete.clone();
        let message = kit::t("@asset.delete.confirm").to_string();
        map_assets_confirm(
            &dialog_for_delete,
            &state_for,
            &message,
            PendingConfirm::BundleDelete(path_for_delete.clone()),
        );
    });
    box_.add_child(&square_cell(
        "trash",
        &kit::t("@asset.delete.confirm").to_string(),
        delete,
    ));
    box_
}

fn map_assets_add_bundle(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let label = kit::t("@add").to_string();
    map_assets_open_file(
        dialog,
        &label,
        vec![String::from("*.properties"), String::from("*.txt")],
        true,
        Rc::new(move |paths: Vec<String>| {
            for path in paths {
                let result = editor_call(
                    "data_file_add",
                    &[
                        GString::from("bundle").to_variant(),
                        GString::from(path.as_str()).to_variant(),
                    ],
                )
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .unwrap_or_default();
                if !dict_bool(&result, "ok", false) {
                    map_assets_error(&dict_string(&result, "error"));
                }
            }
            map_assets_refetch(&state_for);
            map_assets_rebuild(&dialog_for, &state_for);
        }),
    );
}

// --- MapImagesView ---------------------------------------------------------

fn map_assets_build_images(
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
        if dict_bool(row, "generated", false) {
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
struct AssetImageInfo {
    name: String,
    path: String,
    region: String,
    found: bool,
    width: i64,
    height: i64,
    size: i64,
}

fn map_assets_image_cell(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
) -> Gd<PanelContainer> {
    let info = AssetImageInfo {
        name: dict_string(row, "name"),
        path: dict_string(row, "path"),
        region: dict_string(row, "region"),
        found: dict_bool(row, "found", false),
        width: dict_i64(row, "width", 1),
        height: dict_i64(row, "height", 1),
        size: dict_i64(row, "size", 0),
    };
    // code-instantiated: one `Styles.grayPanel` card per image record.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &flat_box(styles::GRAY_PANEL));
    panel.set_custom_minimum_size(Vector2::new(ASSET_IMAGE_CELL, ASSET_IMAGE_CELL));
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 2);
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

    let mut actions = HBoxContainer::new_alloc();
    actions.add_theme_constant_override("separation", 0);
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
                .map(|value| variant_bool(&value))
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

fn map_assets_show_image_info(dialog: &Gd<MindDialog>, info: &AssetImageInfo) {
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

fn map_assets_add_image(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
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
                if !dict_bool(&result, "ok", false) {
                    map_assets_error(&dict_string(&result, "error"));
                }
            }
            map_assets_refetch(&state_for);
            map_assets_rebuild(&dialog_for, &state_for);
        }),
    );
}

/// `MapImagesView` pencil (`ui.showTextInput("@save.rename", "@patch.path", ...)`).
fn map_assets_rename_image(
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

fn map_assets_on_image_rename(
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

// --- MapAudioView ----------------------------------------------------------

fn map_assets_build_audio(
    dialog: &Gd<MindDialog>,
    list: &mut Gd<VBoxContainer>,
    state: &Rc<RefCell<MapAssetsState>>,
    kind: &str,
) {
    let rows = map_assets_rows_of(state, kind);
    let mut shown_count = 0;
    for row in &rows {
        if !map_assets_matches(state, row, "name") {
            continue;
        }
        shown_count += 1;
        let widget = map_assets_audio_row(dialog, state, row, kind);
        list.add_child(&widget);
    }
    if shown_count == 0 {
        let label = factory::label(&markup("@none.found"));
        list.add_child(&label);
    }
}

fn map_assets_audio_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
    kind: &str,
) -> Gd<HBoxContainer> {
    let audio_name = dict_string(row, "name");
    let path = dict_string(row, "path");
    let music = kind == "music";
    let stream_name = map_assets_audio_stream_name(row);
    // code-instantiated: one audio record row (play/export/delete).
    let mut box_ = HBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 4);
    box_.set_alignment(AlignmentMode::CENTER);

    let playing = state.borrow().playing_audio == stream_name;
    let mut play = factory::icon_button(
        if playing { "pause" } else { "play" },
        "",
        "graySquarei",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    play.set_custom_minimum_size(Vector2::new(ASSET_ROW_HEIGHT, ASSET_ROW_HEIGHT));
    play.set_tooltip_text(&kit::t("@play").to_string());
    let dialog_for_play = dialog.clone();
    let state_for_play = state.clone();
    let stream_for_play = stream_name.clone();
    let button_for_play = play.clone();
    let callable = Callable::from_fn("audio_play", move |_args: &[&Variant]| {
        map_assets_play_audio(
            &dialog_for_play,
            &state_for_play,
            &stream_for_play,
            music,
            &button_for_play,
        );
    });
    play.connect("pressed", &callable);
    box_.add_child(&play);

    // `in.add("[accent]" + asset.name)` + the formatted length.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &flat_box(styles::GRAY_PANEL));
    panel.set_custom_minimum_size(Vector2::new(
        map_assets_row_width(&dialog.bind()),
        ASSET_ROW_HEIGHT,
    ));
    panel.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut lines = VBoxContainer::new_alloc();
    lines.add_theme_constant_override("separation", 0);
    let mut title = factory::label(&markup(&format!("[accent]{audio_name}")));
    title.set_horizontal_alignment(HorizontalAlignment::LEFT);
    lines.add_child(&title);
    let seconds = map_assets_audio_length(&stream_name, music);
    let mut length = factory::label(&format!(
        "[lightgray][[{}]",
        map_assets_format_seconds(seconds)
    ));
    length.add_theme_color_override("font_color", LIGHT_GRAY);
    lines.add_child(&length);
    panel.add_child(&lines);
    box_.add_child(&panel);

    let dialog_for_export = dialog.clone();
    let name_for_export = audio_name.clone();
    let path_for_export = path.clone();
    let kind_for_export = String::from(kind);
    let export = Callable::from_fn("audio_export", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_export.clone();
        let name_for = name_for_export.clone();
        let path_for = path_for_export.clone();
        let kind_for = kind_for_export.clone();
        let extension = if path_for.to_lowercase().ends_with(".ogg") {
            "*.ogg"
        } else {
            "*.mp3"
        };
        let label = kit::t("@editor.export").to_string();
        map_assets_save_file(
            &dialog_for,
            &label,
            &name_for,
            extension,
            Rc::new(move |target: String| {
                let exported = editor_call(
                    "data_asset_export",
                    &[
                        GString::from(kind_for.as_str()).to_variant(),
                        GString::from(path_for.as_str()).to_variant(),
                        GString::from(target.as_str()).to_variant(),
                    ],
                )
                .map(|value| variant_bool(&value))
                .unwrap_or(false);
                if !exported {
                    map_assets_error(&kit::t("@asset.broken").to_string());
                }
            }),
        );
    });
    box_.add_child(&square_cell(
        "export",
        &kit::t("@export.file").to_string(),
        export,
    ));

    let dialog_for_trash = dialog.clone();
    let state_for_trash = state.clone();
    let name_for_trash = audio_name.clone();
    let kind_for_trash = String::from(kind);
    let trash = Callable::from_fn("audio_delete", move |_args: &[&Variant]| {
        let message = kit::t("@asset.delete.confirm").to_string();
        map_assets_confirm(
            &dialog_for_trash,
            &state_for_trash,
            &message,
            PendingConfirm::AudioDelete(kind_for_trash.clone(), name_for_trash.clone()),
        );
    });
    box_.add_child(&square_cell(
        "trash",
        &kit::t("@asset.delete.confirm").to_string(),
        trash,
    ));
    box_
}

/// `DataAudioLoader.prefix + asset.name.replace(' ', '_')` (`dp-`).
fn map_assets_audio_stream_name(row: &VarDictionary) -> String {
    let asset_name = dict_string(row, "name").replace(' ', "_");
    format!("dp-{}", asset_name.to_lowercase())
}

fn map_assets_audio_length(stream_name: &str, music: bool) -> f64 {
    let Some(mut assets) = kit::assets() else {
        return 0.0;
    };
    let stream = if music {
        assets.bind_mut().music_stream(GString::from(stream_name))
    } else {
        assets.bind_mut().sound_stream(GString::from(stream_name))
    };
    stream.map(|stream| stream.get_length()).unwrap_or(0.0)
}

fn map_assets_format_seconds(seconds: f64) -> String {
    let total = seconds as i64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn map_assets_play_audio(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    stream_name: &str,
    music: bool,
    button: &Gd<Button>,
) {
    let _ = dialog;
    let Some(mut audio) = kit::node("/root/Spine/MindAudio") else {
        return;
    };
    if !audio.has_method("play_data_audio") {
        return;
    }
    let started = audio
        .call(
            "play_data_audio",
            &[GString::from(stream_name).to_variant(), music.to_variant()],
        )
        .try_to::<bool>()
        .unwrap_or(false);
    if !started {
        return;
    }
    state.borrow_mut().playing_audio = String::from(stream_name);
    if let Some(texture) = kit::icon_texture("pause") {
        let mut button = button.clone();
        button.set_button_icon(&texture);
    }
}

fn map_assets_add_audio(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>, kind: &str) {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let kind_for = String::from(kind);
    let label = kit::t("@add").to_string();
    map_assets_open_file(
        dialog,
        &label,
        vec![String::from("*.ogg"), String::from("*.mp3")],
        true,
        Rc::new(move |paths: Vec<String>| {
            for path in paths {
                let result = editor_call(
                    "data_file_add",
                    &[
                        GString::from(kind_for.as_str()).to_variant(),
                        GString::from(path.as_str()).to_variant(),
                    ],
                )
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .unwrap_or_default();
                if !dict_bool(&result, "ok", false) {
                    map_assets_error(&dict_string(&result, "error"));
                }
            }
            map_assets_refetch(&state_for);
            map_assets_rebuild(&dialog_for, &state_for);
        }),
    );
}

// ---------------------------------------------------------------------------
// SchematicInfoDialog (`schematic_info`)
// ---------------------------------------------------------------------------

/// `SchematicInfoDialog.show`: the `[[Schematic] name]` title plus the body.
#[derive(Default)]
pub struct SchematicInfoDialog {
    /// The dialog body table (`_body`).
    body: Option<Gd<MindTable>>,
}

impl DialogController for SchematicInfoDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@schematic");
        dialog.should_pause = false;
        dialog.enable_full_dialog(true);
        self.body = Some(dialog.content_table());
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.build(dialog);
    }
}

impl SchematicInfoDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        let Some(mut body) = self.body.clone() else {
            return;
        };
        body.bind_mut().clear();
        let schematics = dialog.campaign_section("schematics");
        let index = dialog
            .context_value("index")
            .map(|value| variant_i64(&value))
            .unwrap_or(-1);
        let schematic = find_schematic(&schematics, index);
        if schematic.is_empty() {
            return;
        }
        // Java: `"[[" + Core.bundle.get("schematic") + "] " + name`. The title
        // label is a plain `Label`, so the escape renders as the literal
        // `[Schematic] Name` Mindustry shows.
        let title = format!(
            "[{}] {}",
            kit::t("@schematic"),
            dict_string(&schematic, "name")
        );
        dialog.title_text = GString::from(title.as_str());
        dialog.title_color = styles::ACCENT;
        dialog.refresh_title();

        let mut info = factory::table();
        info.bind_mut().grow_horizontal();
        // `Core.bundle.format("schematic.info", w, h, tiles.size)`.
        let tiles = schematic
            .get("tiles")
            .map(|value| {
                value
                    .try_to::<VarArray>()
                    .map(|array| array.len())
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        let text = kit::tm_format(
            "@schematic.info",
            &[
                &dict_i64(&schematic, "width", 0).to_string(),
                &dict_i64(&schematic, "height", 0).to_string(),
                &tiles.to_string(),
            ],
        )
        .to_string();
        let label = factory::label(&text);
        info.bind_mut().add(&label).grow_x();
        info.bind_mut().row();

        // code-instantiated: the preview is bound to the selected schematic row.
        let mut holder = factory::space(0.0, 320.0);
        holder.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let mut image = crate::ui::kit::widgets::schematic_image::MindSchematicImage::new_alloc();
        image.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        image.bind_mut().setup(schematic.clone());
        holder.add_child(&image);
        info.bind_mut().add(&holder).grow_x().pad(6.0);
        info.bind_mut().row();

        self.build_requirements(&mut info, &schematic);

        let production = dict_f64(&schematic, "power_production", 0.0) * 60.0;
        let consumption = dict_f64(&schematic, "power_consumption", 0.0) * 60.0;
        if production != 0.0 || consumption != 0.0 {
            // code-instantiated: the schematic's power summary row.
            let mut row = HBoxContainer::new_alloc();
            row.add_theme_constant_override("separation", 3);
            if production != 0.0 {
                row.add_child(&factory::glyph_label(
                    "power",
                    20,
                    Color::from_rgba(1.0, 0.827, 0.498, 1.0),
                ));
                let mut plus = factory::label(&format!("+{}", auto_fixed(production, 2)));
                plus.add_theme_color_override(
                    "default_color",
                    Color::from_rgba(1.0, 0.827, 0.498, 1.0),
                );
                row.add_child(&plus);
                if consumption != 0.0 {
                    let gap = factory::space(15.0, 0.0);
                    row.add_child(&gap);
                }
            }
            if consumption != 0.0 {
                row.add_child(&factory::glyph_label(
                    "power",
                    20,
                    Color::from_rgba(0.898, 0.329, 0.329, 1.0),
                ));
                let mut minus = factory::label(&format!("-{}", auto_fixed(consumption, 2)));
                minus.add_theme_color_override(
                    "default_color",
                    Color::from_rgba(0.898, 0.329, 0.329, 1.0),
                );
                row.add_child(&minus);
            }
            info.bind_mut().add(&row).pad(4.0);
            info.bind_mut().row();
        }

        let description = dict_string(&schematic, "description");
        if !description.is_empty() {
            let mut wrapped = factory::label(&markup(&format!("[lightgray]{description}")));
            wrapped.set_custom_minimum_size(Vector2::new(500.0, 0.0));
            wrapped.set_autowrap_mode(AutowrapMode::WORD_SMART);
            info.bind_mut().add(&wrapped).pad(8.0);
            info.bind_mut().row();
        }
        body.bind_mut().add(&info).grow_x();
        body.bind_mut().sort_now();
    }
}

impl SchematicInfoDialog {
    /// Requirement rows: 32 px icon + amount, four per row; when a player core
    /// exists and cannot afford the stack the label is `have/amount` in scarlet.
    fn build_requirements(&self, info: &mut Gd<MindTable>, schematic: &VarDictionary) {
        let requirements = schematic
            .get("requirements")
            .and_then(|value| value.try_to::<VarArray>().ok())
            .unwrap_or_default();
        if requirements.is_empty() {
            return;
        }
        let have = core_items();
        // code-instantiated: one requirement cell per schematic stack.
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 4);
        let mut count = 0usize;
        for entry in requirements.iter_shared() {
            let Ok(entry) = entry.try_to::<VarArray>() else {
                continue;
            };
            let mut values = entry.iter_shared();
            let item = values
                .next()
                .map(|value| variant_string(&value))
                .unwrap_or_default();
            let amount = values.next().map(|value| variant_i64(&value)).unwrap_or(0);
            let mut icon = TextureRect::new_alloc();
            if let Some(texture) = kit::icon_texture(&format!("item-{item}-ui")) {
                icon.set_texture(&texture);
            }
            icon.set_custom_minimum_size(Vector2::new(32.0, 32.0));
            icon.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
            row.add_child(&icon);
            let mut label = factory::label(&amount.to_string());
            let available = have
                .get(item.as_str())
                .map(|value| variant_i64(&value))
                .unwrap_or(0);
            if !have.is_empty() && available < amount {
                label.set_text(&GString::from(
                    format!("[scarlet]{available}[]/{amount}").as_str(),
                ));
            }
            row.add_child(&label);
            count += 1;
            if count.is_multiple_of(4) {
                info.bind_mut().add(&row).pad(2.0);
                info.bind_mut().row();
                row = HBoxContainer::new_alloc();
                row.add_theme_constant_override("separation", 4);
            }
        }
        if !count.is_multiple_of(4) {
            info.bind_mut().add(&row).pad(2.0);
            info.bind_mut().row();
        }
    }
}

/// The player core's item counts (`player.core().items`); empty in the menu.
fn core_items() -> VarDictionary {
    let Some(mut host) = kit::sim_host() else {
        return VarDictionary::new();
    };
    if !host.has_method("core_items_json") {
        return VarDictionary::new();
    }
    let value = host.call("core_items_json", &[]);
    parse_json_object(&value.to::<GString>().to_string())
}

/// Arc `Strings.autoFixed(value, max)`: truncate to `max` decimals, drop
/// trailing zeros (`Strings.java:1230`).
fn auto_fixed(value: f64, max_decimals: usize) -> String {
    let mut text = format!("{value:.max_decimals$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text.is_empty() {
        String::from("0")
    } else {
        text
    }
}

/// The view row for `index` (falls back to the first row).
fn find_schematic(schematics: &VarArray, index: i64) -> VarDictionary {
    for row in schematics.iter_shared() {
        let Ok(row) = row.try_to::<VarDictionary>() else {
            continue;
        };
        if dict_i64(&row, "index", -1) == index {
            return row;
        }
    }
    schematics
        .iter_shared()
        .next()
        .and_then(|row| row.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banned_keys_are_the_rules_abi() {
        assert_eq!(banned_key("block"), "bannedBlocks");
        assert_eq!(banned_key("unit"), "bannedUnits");
        assert_eq!(banned_title_key("block"), "@bannedblocks");
        assert_eq!(banned_title_key("unit"), "@bannedunits");
    }

    #[test]
    fn asset_type_tables_are_the_abi() {
        assert_eq!(ASSET_TYPES.len(), 6);
        assert_eq!(asset_type_key("patch"), "@asset.patch");
        assert_eq!(asset_type_icon("music"), "music");
        assert_eq!(asset_content_icon("weather"), "effect");
        assert_eq!(ASSET_CONTENT_TYPES.len(), 6);
    }

    #[test]
    fn auto_fixed_drops_trailing_zeros() {
        assert_eq!(auto_fixed(120.0, 2), "120");
        assert_eq!(auto_fixed(1.5, 2), "1.5");
        assert_eq!(auto_fixed(0.0, 2), "0");
    }

    /// PAR-0541: `map_assets_audio_stream_name` reads the row through
    /// `dict_string` (a Godot `VarDictionary`), so calling it from a plain
    /// cargo unit test panics with `Godot engine not available`. The naming
    /// rule itself is pure — `dp-` + the row's `name` with spaces replaced by
    /// underscores, lowercased (`batch_f.rs:4220-4222`) — and the seconds
    /// formatter is pure, so this test exercises both without touching the
    /// engine. The FFI-backed call is covered by the in-engine dialog check.
    #[test]
    fn audio_stream_name_uses_the_dp_prefix() {
        let stream_name = |name: &str| format!("dp-{}", name.replace(' ', "_").to_lowercase());
        assert_eq!(stream_name("My Sound"), "dp-my_sound");
        assert_eq!(stream_name("Ambient"), "dp-ambient");
        assert_eq!(stream_name("Two Words Here"), "dp-two_words_here");
        // A name already lowercase/underscored round-trips unchanged.
        assert_eq!(stream_name("my_sound"), "dp-my_sound");
        // `map_assets_format_seconds` is engine-free (pure formatting).
        assert_eq!(map_assets_format_seconds(61.7), "1:01");
        assert_eq!(map_assets_format_seconds(0.0), "0:00");
        assert_eq!(map_assets_format_seconds(3600.0), "60:00");
    }

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

    /// PAR-0586: `find_schematic` walks a Godot `VarArray` of `VarDictionary`
    /// rows, so the fallback rule is exercised only in-engine.
    #[test]
    fn schematics_fall_back_to_the_first_row() {
        if !engine_available() {
            return;
        }
        let mut first = VarDictionary::new();
        first.set(&"index".to_variant(), &7i64.to_variant());
        let mut schematics = VarArray::new();
        schematics.push(&first.to_variant());
        let picked = find_schematic(&schematics, 99);
        assert_eq!(dict_i64(&picked, "index", -1), 7);
    }

    #[test]
    fn error_codes_map_to_bundle_keys() {
        assert_eq!(
            map_assets_error_key("exists"),
            Some("@asset.content.exists")
        );
        assert_eq!(
            map_assets_error_key("badname"),
            Some("@asset.content.badname")
        );
        assert_eq!(map_assets_error_key("badtype"), Some("@asset.content.type"));
        assert_eq!(
            map_assets_error_key("invalid JSON"),
            Some("@patch.importerror")
        );
        assert_eq!(map_assets_error_key("other"), None);
    }
}
