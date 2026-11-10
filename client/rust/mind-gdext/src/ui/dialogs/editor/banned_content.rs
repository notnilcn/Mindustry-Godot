// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The banned content dialog controller.
//! Upstream: editor/BannedContentDialog.java

use crate::gdutil::variant::{dict_i64_or, dict_text_or, parse_object, to_text_lenient};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::editor::util::*;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindScroll, styles};
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::SizeFlags;
use godot::classes::{
    BoxContainer, Button, CenterContainer, ColorRect, GridContainer, HBoxContainer, LineEdit,
    VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "banned_content" => Some(Box::new(BannedContentDialog::default())),
        _ => None,
    }
}

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
        dialog.add_default_close_button();
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
        let payload = parse_object(&value.to::<GString>().to_string());
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
                .map(|entry| to_text_lenient(&entry))
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
    parse_object(&to_text_lenient(&rules))
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
    let mut search_row = factory::hbox(10);
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
    kit::clear_children(&mut selected_grid);
    kit::clear_children(&mut deselected_grid);
    let filtered = banned_filtered_rows(state);
    let banned = state.borrow().banned.clone();
    let mut banned_rows: Vec<VarDictionary> = Vec::new();
    let mut available_rows: Vec<VarDictionary> = Vec::new();
    for row in filtered {
        if banned.contains(&dict_text_or(&row, "name")) {
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
        let content_name = dict_text_or(row, "name");
        let icon = dict_text_or(row, "ui_icon");
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
        if !category.is_empty() && dict_text_or(&row, "category") != category {
            continue;
        }
        if !search.is_empty() {
            // `content.localizedName.toLowerCase().contains(contentSearch)`.
            let localized = banned_localized(state, &dict_text_or(&row, "name"));
            if !localized.to_lowercase().contains(&search) {
                continue;
            }
        }
        out.push(row);
    }
    out.sort_by_key(|row| dict_i64_or(row, "id", 0));
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
            let name = dict_text_or(row, "name");
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
}
