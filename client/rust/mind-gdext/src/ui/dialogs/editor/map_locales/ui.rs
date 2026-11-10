// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Card, icon-grid and sub-dialog builders of the map locales dialog.
//! Upstream: editor/MapLocalesDialog.java

use godot::builtin::Side;
use godot::classes::Button;
use godot::classes::ColorRect;
use godot::classes::Control;
use godot::classes::DisplayServer;
use godot::classes::HBoxContainer;
use godot::classes::Label;
use godot::classes::LineEdit;
use godot::classes::PanelContainer;
use godot::classes::StyleBox;
use godot::classes::StyleBoxFlat;
use godot::classes::TextureRect;
use godot::classes::VBoxContainer;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::LayoutPreset;
use godot::classes::control::MouseFilter;
use godot::classes::control::SizeFlags;
use godot::classes::texture_rect::ExpandMode;
use godot::classes::texture_rect::StretchMode;
use godot::global::HorizontalAlignment;
use godot::global::VerticalAlignment;
use godot::obj::NewAlloc;
use godot::obj::NewGd;
use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::MindTable;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::factory;
use mind_core::maps::locales::ensure_locale;
use mind_core::maps::locales::read_locale;
use mind_core::maps::locales::write_locale;

use super::super::util_e::*;

use super::model::{
    LOCALE_APPLY_ALL_META, LOCALE_CARD_WIDTH, LOCALE_COLLAPSED_META, LOCALE_CONFIRM_BOUND_META,
    LOCALE_DUPLICATE_COLOR, LOCALE_FIELD_KEY_META, LOCALE_FILTER_OVERLAY, LOCALE_GRAY,
    LOCALE_PENDING_DELETE_META, LOCALE_PENDING_SAVE_META, LOCALE_PLACEHOLDER_VALUE,
    LOCALE_REQUEST_MAIN, LOCALE_REQUEST_TABLES, LOCALE_RESIZE_BOUND_META,
    LOCALE_RESIZE_COLUMNS_META, LOCALE_SAVED_META, LOCALE_SEARCH_META, LOCALE_SEARCH_VALUE_META,
    LOCALE_SELECTED_META, LOCALE_SHOW_CORRECT_META, LOCALE_SHOW_MISSING_META,
    LOCALE_SHOW_SAME_META, LOCALE_SUB_OVERLAY, LOCALE_TECH_BLUE, LocaleStatus, locale_add_to_other,
    locale_default_locale, locale_display_name, locale_icon_columns, locale_key_valid,
    locale_main_columns, locale_mark_dirty, locale_matches_search, locale_property_status,
    locale_read_bundles, locale_remove_locale, locale_remove_property, locale_rename_key,
    locale_saved_state, locale_selected, locale_set_state, locale_show_flags, locale_state,
    locale_status_color, locale_status_visible, locale_sync_apply, locale_view_columns,
    locale_viewport_width, locale_write_bundles, locale_write_tags,
};

// -- widgets ----------------------------------------------------------------

/// One status-tinted `Tex.whitePane` card frame. The white-pane texture is a
/// 5 px white rounded border around an empty centre, so `Table.setColor`
/// (`MapLocalesDialog.updateCard`) only shows on the border.
fn locale_card_frame(color: Color) -> Gd<StyleBox> {
    // code-instantiated: one tinted white-pane frame per card/filter row.
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        style.set_border_width(side, 4);
    }
    style.set_border_color(color);
    style.set_corner_radius_all(4);
    style.set_content_margin_all(6.0);
    style.upcast()
}

fn locale_update_card(card: &Gd<PanelContainer>, color: Color) {
    let mut card = card.clone();
    card.add_theme_stylebox_override("panel", &locale_card_frame(color));
}

/// A `Pal.gray` 3 px rule (`main.image().color(Pal.gray).height(3f)`).
pub(super) fn locale_rule(color: Color) -> Gd<ColorRect> {
    // code-instantiated: the pane rules are per-build presentation nodes.
    let mut rule = ColorRect::new_alloc();
    rule.set_color(color);
    rule.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    rule.set_h_size_flags(SizeFlags::EXPAND_FILL);
    rule.set_mouse_filter(MouseFilter::IGNORE);
    rule
}

/// A glyph-font label for a raw icon-font string (content/team emoji or an
/// `Iconc` code); `factory::glyph_label` only resolves Java alias names.
fn locale_glyph_char(text: &str, size: i32) -> Gd<Label> {
    // code-instantiated: the icon gallery draws resolved PUA characters.
    let mut label = Label::new_alloc();
    label.set_text(text);
    let mut control: Gd<Control> = label.clone().upcast();
    kit::icons::apply_icon_font(&mut control);
    label.add_theme_font_size_override("font_size", size);
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_horizontal_alignment(HorizontalAlignment::CENTER);
    label.set_mouse_filter(MouseFilter::IGNORE);
    label
}

/// One glyph-only toggle (`button(Icon.x, Styles.emptyTogglei, ...)`): the
/// caller connects `pressed` and swaps the label for the checked state.
pub(super) fn locale_glyph_toggle(icon: &str, style: &str, size: f32) -> (Gd<Button>, Gd<Label>) {
    // code-instantiated: one glyph-only toggle per card/filter row.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation(style);
    button.set_toggle_mode(true);
    button.set_custom_minimum_size(Vector2::new(size, size));
    let mut glyph = factory::glyph_label(icon, 30, Color::WHITE);
    glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    glyph.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&glyph);
    (button, glyph)
}

/// One `filterStyle` status toggle (`MapLocalesDialog.java:580-617`): a
/// white-pane frame in the status colour, the label on the left and the
/// `ok`/`cancel` indicator on the right.
fn locale_filter_toggle(text: &str, color: Color, checked: bool) -> (Gd<Button>, Gd<Label>) {
    // code-instantiated: one 450x100 status toggle per filter.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("flatTogglet");
    button.set_toggle_mode(true);
    button.set_pressed_no_signal(checked);
    button.set_custom_minimum_size(Vector2::new(450.0, 100.0));
    let frame = locale_card_frame(color);
    for slot in ["normal", "hover", "pressed", "disabled"] {
        button.add_theme_stylebox_override(slot, &frame);
    }
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, 12.0);
    row.set_offset(Side::RIGHT, -12.0);
    row.add_theme_constant_override("separation", 8);
    row.set_mouse_filter(MouseFilter::IGNORE);
    let mut label = factory::label(text);
    label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    row.add_child(&as_control(&label));
    let glyph_text = kit::glyph(if checked { "ok" } else { "cancel" });
    let glyph = locale_glyph_char(&glyph_text.to_string(), 28);
    row.add_child(&glyph);
    button.add_child(&as_control(&row));
    (button, glyph)
}

/// One 48 px icon-gallery cell (`iconsTable`, `MapLocalesDialog.java:644-706`).
fn locale_icon_button(
    node: &Gd<MindDialog>,
    layer: &Gd<ColorRect>,
    key: &str,
    value: &str,
    entry: &LocaleIconEntry,
) -> Gd<Button> {
    // code-instantiated: one data-driven icon cell per gallery entry.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("flati");
    button.set_custom_minimum_size(Vector2::new(48.0, 48.0));
    button.set_tooltip_text(&GString::from(entry.tooltip.as_str()));
    if let Some(texture) = kit::icon_texture(&entry.region) {
        let mut icon = TextureRect::new_alloc();
        icon.set_texture(&texture);
        icon.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
        icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        icon.set_mouse_filter(MouseFilter::IGNORE);
        button.add_child(&icon);
    } else if !entry.glyph.is_empty() {
        let mut glyph = locale_glyph_char(&entry.glyph, 28);
        glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        button.add_child(&glyph);
    }
    let target = node.clone();
    let layer = layer.clone();
    let key = key.to_string();
    let base = value.to_string();
    let glyph = entry.glyph.clone();
    let callable = Callable::from_fn("locales_icon_pick", move |_args: &[&Variant]| {
        let selected = locale_selected(&target);
        let mut state = locale_state(&target);
        if let Some(props) = state.0.get_mut(&selected) {
            props.insert(key.clone(), format!("{base}{glyph}"));
        }
        locale_set_state(&target, &state);
        locale_mark_dirty(&target);
        if layer.is_instance_valid() {
            let mut layer = layer.clone();
            layer.queue_free();
        }
        request_refresh(&target, LOCALE_REQUEST_MAIN);
    });
    button.connect("pressed", &callable);
    button
}

/// One `@locales` content-icon gallery entry: the appended glyph, the atlas
/// region to draw and the tooltip/search text.
struct LocaleIconEntry {
    tooltip: String,
    glyph: String,
    region: String,
}

/// A `locale:`-free name search for the icon gallery: spaces are stripped and
/// both sides lowercased, matching `iconsTable` (`MapLocalesDialog.java:661`).
fn locale_icon_matches(search: &str, candidate: &str) -> bool {
    search.is_empty()
        || candidate
            .replace(' ', "")
            .to_lowercase()
            .contains(&search.to_lowercase())
}

/// `UnlockableContent.localizedName` for a `database_json` row (the label is a
/// bundle key when the localized name is missing).
fn locale_localized_name(text: &str) -> String {
    if text.is_empty() {
        String::new()
    } else {
        kit::t(text).to_string()
    }
}

/// `MindAssets.unicode_str(name)` (`Fonts.getUnicodeStr`), empty when absent.
fn locale_unicode(name: &str) -> String {
    let Some(assets) = kit::assets() else {
        return String::new();
    };
    assets.bind().unicode_str(GString::from(name)).to_string()
}

/// `Iconc.codes` iteration for the `@locales.addicon` gallery
/// (`MapLocalesDialog.java:659-675`).
fn locale_icon_code_entries(search: &str) -> Vec<LocaleIconEntry> {
    let Some(assets) = kit::assets() else {
        return Vec::new();
    };
    let assets = assets.bind();
    let names = assets.icon_names();
    let mut entries = Vec::new();
    for name in names.as_slice() {
        let name = name.to_string();
        if !locale_icon_matches(search, &name) {
            continue;
        }
        let glyph = assets.unicode_str(GString::from(name.as_str())).to_string();
        if glyph.is_empty() {
            continue;
        }
        let region = assets
            .icon_texture(GString::from(name.as_str()))
            .to_string();
        entries.push(LocaleIconEntry {
            tooltip: name,
            glyph,
            region,
        });
    }
    entries
}

/// The unlockable `database_json` entries, one row per content
/// (`ContentType` iteration for the icon gallery, `MapLocalesDialog.java:680`).
/// Fetched once per gallery rebuild: serializing the content registry is the
/// expensive part, and the gallery needs all five types.
fn locale_database_rows() -> Vec<VarDictionary> {
    let Some(mut campaign) = kit::campaign() else {
        return Vec::new();
    };
    if !campaign.has_method("database_json") {
        return Vec::new();
    }
    let view = json_object(&campaign.call("database_json", &[]).to::<GString>());
    let mut rows = Vec::new();
    for category in dict_rows(&dict_array(&view, "categories")) {
        for tag in dict_rows(&dict_array(&category, "tags")) {
            rows.extend(dict_rows(&dict_array(&tag, "entries")));
        }
    }
    rows
}

/// One content type's unlocked entries (`getBy(type).select(u -> ...)`,
/// `MapLocalesDialog.java:680-692`).
fn locale_content_icon_section(
    rows: &[VarDictionary],
    type_name: &str,
    search: &str,
) -> Vec<LocaleIconEntry> {
    let mut entries = Vec::new();
    for row in rows {
        if dict_text(row, "content_type") != type_name || !dict_bool(row, "unlocked") {
            continue;
        }
        let name = dict_text(row, "name");
        let localized = locale_localized_name(&dict_text(row, "localized"));
        if !locale_icon_matches(search, &localized) {
            continue;
        }
        let glyph = locale_unicode(&name);
        if glyph.is_empty() {
            continue;
        }
        entries.push(LocaleIconEntry {
            tooltip: localized,
            glyph,
            region: format!("{type_name}-{name}-ui"),
        });
    }
    entries
}

/// `Team.baseTeams` with `team.emoji` (`MapLocalesDialog.java:695-706`); the
/// editor's palette teams are the six base teams.
fn locale_team_icon_entries(search: &str) -> Vec<LocaleIconEntry> {
    let mut entries = Vec::new();
    for row in palette_teams() {
        let name = dict_text(&row, "name");
        if name.is_empty() {
            continue;
        }
        let localized = locale_localized_name(&format!("team.{name}.name"));
        if !locale_icon_matches(search, &localized) {
            continue;
        }
        let glyph = locale_unicode(&name);
        if glyph.is_empty() {
            continue;
        }
        entries.push(LocaleIconEntry {
            tooltip: localized,
            glyph,
            region: format!("team-{name}"),
        });
    }
    entries
}

/// The `@locales.addicon` gallery sections in Java order: every `Iconc` code,
/// then `item`/`block`/`liquid`/`status`/`unit` content, then the base teams.
/// Empty sections are dropped (`if(all.size > 0) table.image()...`).
fn locale_icon_sections(search: &str) -> Vec<Vec<LocaleIconEntry>> {
    let mut sections = Vec::new();
    let codes = locale_icon_code_entries(search);
    if !codes.is_empty() {
        sections.push(codes);
    }
    let rows = locale_database_rows();
    for type_name in ["item", "block", "liquid", "status", "unit"] {
        let entries = locale_content_icon_section(&rows, type_name, search);
        if !entries.is_empty() {
            sections.push(entries);
        }
    }
    let teams = locale_team_icon_entries(search);
    if !teams.is_empty() {
        sections.push(teams);
    }
    sections
}

/// Fills the icon gallery (`iconsTable`); called on every search change.
fn locale_fill_icon_grid(
    grid: &Gd<MindTable>,
    node: &Gd<MindDialog>,
    layer: &Gd<ColorRect>,
    key: &str,
    value: &str,
    search: &str,
) {
    let mut grid = grid.clone();
    let mut bound = grid.bind_mut();
    bound.clear();
    let columns = locale_icon_columns(locale_viewport_width(node));
    for section in locale_icon_sections(search) {
        bound
            .add(&locale_rule(styles::ACCENT))
            .colspan(columns as i32)
            .grow_x();
        bound.row();
        for (index, entry) in section.iter().enumerate() {
            let button = locale_icon_button(node, layer, key, value, entry);
            bound.add(&button).size(48.0);
            if (index + 1).is_multiple_of(columns) {
                bound.row();
            }
        }
        if !section.len().is_multiple_of(columns) {
            bound.row();
        }
    }
}

/// The `Callable` that rebuilds the main card pane
/// (`filterDialog(this::buildMain)`).
pub(super) fn locale_main_refresh(node: &Gd<MindDialog>) -> Callable {
    let target = node.clone();
    Callable::from_fn("locales_refresh_main", move |_args: &[&Variant]| {
        request_refresh(&target, LOCALE_REQUEST_MAIN);
    })
}

/// The key field's last committed property key
/// (`MapLocalesDialog.buildMain`'s `propKey[0]`).
fn locale_field_key(field: &Gd<LineEdit>) -> String {
    if !field.has_meta(LOCALE_FIELD_KEY_META) {
        return String::new();
    }
    variant_text(&field.get_meta(LOCALE_FIELD_KEY_META))
}

fn locale_set_field_key(field: &Gd<LineEdit>, key: &str) {
    let mut field = field.clone();
    field.set_meta(LOCALE_FIELD_KEY_META, &GString::from(key).to_variant());
}

/// One property card (`colTables[i].table(Tex.whitePane, t -> ...)`,
/// `MapLocalesDialog.java:231-305`): collapse toggle, key field, delete, edit
/// actions and the collapsible value area.
pub(super) fn locale_build_card(
    node: &Gd<MindDialog>,
    key: &str,
    value: &str,
    status: LocaleStatus,
) -> Gd<Control> {
    // code-instantiated: one card per property, rebuilt on every `buildMain`.
    let mut card = PanelContainer::new_alloc();
    card.add_theme_stylebox_override("panel", &locale_card_frame(locale_status_color(status)));
    card.set_custom_minimum_size(Vector2::new(LOCALE_CARD_WIDTH, 0.0));
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 4);
    card.add_child(&column);
    let mut row = HBoxContainer::new_alloc();
    column.add_child(&row);
    let collapsed = meta_bool(node, LOCALE_COLLAPSED_META);
    let is_shown = !collapsed;
    let (mut collapse, collapse_glyph) = locale_glyph_toggle(
        if is_shown { "up-open" } else { "down-open" },
        "emptyTogglei",
        35.0,
    );
    collapse.set_pressed_no_signal(is_shown);
    row.add_child(&collapse);
    let mut key_field = text_field(key, 64);
    key_field.set_custom_minimum_size(Vector2::new(LOCALE_CARD_WIDTH - 125.0, 0.0));
    key_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    locale_set_field_key(&key_field, key);
    row.add_child(&key_field);
    let mut remove = factory::glyph_button("trash", "", "emptyi");
    remove.set_custom_minimum_size(Vector2::new(35.0, 35.0));
    row.add_child(&remove);
    let mut edit = factory::glyph_button("edit", "", "emptyi");
    edit.set_custom_minimum_size(Vector2::new(35.0, 35.0));
    row.add_child(&edit);
    let mut area = text_area(value);
    area.set_custom_minimum_size(Vector2::new(0.0, 140.0));
    area.set_h_size_flags(SizeFlags::EXPAND_FILL);
    area.set_visible(is_shown);
    column.add_child(&area);

    // Collapse toggle: the value area hides and the glyph flips
    // (`t.button(Icon.downOpen, Styles.emptyTogglei, () -> shown[0] = !shown[0])`).
    let collapse_state = collapse.clone();
    let area_for_toggle = area.clone();
    let glyph_for_toggle = collapse_glyph;
    let callable = Callable::from_fn("locales_card_collapse", move |_args: &[&Variant]| {
        let visible = collapse_state.is_pressed();
        let mut area = area_for_toggle.clone();
        area.set_visible(visible);
        let mut glyph = glyph_for_toggle.clone();
        glyph.set_text(&kit::glyph(if visible { "up-open" } else { "down-open" }));
    });
    collapse.connect("pressed", &callable);

    // Key field: reject `=`/`:` (Arc's CharFilter), highlight a duplicate in
    // red without renaming, otherwise rename in the selected locale or all.
    let card_for_key = card.clone();
    let node_for_key = node.clone();
    let field_for_key = key_field.clone();
    let value_for_key = value.to_string();
    let callable = Callable::from_fn("locales_card_key", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|entry| entry.to::<GString>().to_string())
            .unwrap_or_default();
        if !locale_key_valid(&text) {
            let previous = locale_field_key(&field_for_key);
            let mut field = field_for_key.clone();
            field.set_text(&GString::from(previous.as_str()));
            return;
        }
        let previous = locale_field_key(&field_for_key);
        let selected = locale_selected(&node_for_key);
        let mut state = locale_state(&node_for_key);
        let duplicate = state
            .0
            .get(&selected)
            .is_some_and(|props| props.contains_key(&text));
        if duplicate {
            locale_update_card(&card_for_key, LOCALE_DUPLICATE_COLOR);
            return;
        }
        let apply_all = meta_bool(&node_for_key, LOCALE_APPLY_ALL_META);
        locale_rename_key(
            &mut state,
            &selected,
            &previous,
            &text,
            &value_for_key,
            apply_all,
        );
        locale_set_state(&node_for_key, &state);
        locale_mark_dirty(&node_for_key);
        locale_set_field_key(&field_for_key, &text);
        let current = state
            .0
            .get(&selected)
            .and_then(|props| props.get(&text))
            .cloned()
            .unwrap_or_default();
        let status = locale_property_status(&state, &text, Some(&current), &selected, false);
        locale_update_card(&card_for_key, locale_status_color(status));
    });
    key_field.connect("text_changed", &callable);

    // Value area: `props.put(propKey[0], v); updateCard(...)`.
    let card_for_value = card.clone();
    let node_for_value = node.clone();
    let field_for_value = key_field.clone();
    let area_for_value = area.clone();
    let callable = Callable::from_fn("locales_card_value", move |_args: &[&Variant]| {
        let key = locale_field_key(&field_for_value);
        let text = area_for_value.get_text().to_string();
        let selected = locale_selected(&node_for_value);
        let mut state = locale_state(&node_for_value);
        if let Some(props) = state.0.get_mut(&selected) {
            props.insert(key.clone(), text.clone());
        }
        locale_set_state(&node_for_value, &state);
        locale_mark_dirty(&node_for_value);
        let status = locale_property_status(&state, &key, Some(&text), &selected, false);
        locale_update_card(&card_for_value, locale_status_color(status));
    });
    area.connect("text_changed", &callable);

    // Delete: the key from every bundle or only the selected one.
    let node_for_remove = node.clone();
    let field_for_remove = key_field.clone();
    let callable = Callable::from_fn("locales_card_remove", move |_args: &[&Variant]| {
        let key = locale_field_key(&field_for_remove);
        let selected = locale_selected(&node_for_remove);
        let mut state = locale_state(&node_for_remove);
        let apply_all = meta_bool(&node_for_remove, LOCALE_APPLY_ALL_META);
        locale_remove_property(&mut state, &selected, &key, apply_all);
        locale_set_state(&node_for_remove, &state);
        locale_mark_dirty(&node_for_remove);
        request_refresh(&node_for_remove, LOCALE_REQUEST_MAIN);
    });
    remove.connect("pressed", &callable);

    // More actions (`propEditDialog`).
    let node_for_edit = node.clone();
    let field_for_edit = key_field.clone();
    let callable = Callable::from_fn("locales_card_edit", move |_args: &[&Variant]| {
        let key = locale_field_key(&field_for_edit);
        let selected = locale_selected(&node_for_edit);
        let state = locale_state(&node_for_edit);
        let value = state
            .0
            .get(&selected)
            .and_then(|props| props.get(&key))
            .cloned()
            .unwrap_or_default();
        locale_show_prop_edit(&node_for_edit, &key, &value);
    });
    edit.connect("pressed", &callable);
    card.upcast()
}

/// Whether the property rollback button is enabled
/// (`MapLocalesDialog.java:405-414`): `lastSaved` has the locale + key and the
/// current value differs.
fn locale_rollback_available(node: &Gd<MindDialog>, key: &str) -> bool {
    let selected = locale_selected(node);
    let saved = locale_saved_state(node);
    let Some(saved_value) = saved.0.get(&selected).and_then(|props| props.get(key)) else {
        return false;
    };
    let state = locale_state(node);
    state.0.get(&selected).and_then(|props| props.get(key)) != Some(saved_value)
}

/// `ui.showConfirm(title, text, runnable)`: the port's prompt carries one text
/// block, so title and message are joined (`@confirm` + the question).
pub(super) fn locale_ask_confirm(text: &str) -> bool {
    let Some(mut ui) = kit::ui() else {
        return false;
    };
    if !ui.has_method("show_confirm") {
        return false;
    }
    let _ = ui.call("show_confirm", &[GString::from(text).to_variant()]);
    true
}

/// `_connect_confirm`: `MindUi.confirm_result` -> the pending save/delete
/// action (the signal is emitted inside a `MindUi` borrow, so the rebuild is
/// deferred).
pub(super) fn locale_bind_confirm(node: &Gd<MindDialog>) {
    if node.has_meta(LOCALE_CONFIRM_BOUND_META) {
        return;
    }
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_signal("confirm_result") {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("locales_confirm_result", move |args: &[&Variant]| {
        let confirmed = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        let mut meta = target.clone();
        let pending_save = meta_bool(&meta, LOCALE_PENDING_SAVE_META);
        let pending_delete = meta_text(&meta, LOCALE_PENDING_DELETE_META);
        set_meta_bool(&mut meta, LOCALE_PENDING_SAVE_META, false);
        set_meta_text(&mut meta, LOCALE_PENDING_DELETE_META, "");
        if !confirmed {
            return;
        }
        if pending_save {
            locale_write_tags(&target);
        }
        if pending_delete.is_empty() {
            return;
        }
        locale_delete_locale(&target, &pending_delete);
        request_refresh_deferred(&target, LOCALE_REQUEST_TABLES);
    });
    ui.connect("confirm_result", &callable);
    node.clone()
        .set_meta(LOCALE_CONFIRM_BOUND_META, &true.to_variant());
}

/// The `@locales.deletelocale` answer (`MapLocalesDialog.java:190-195`):
/// remove the bundle, then select the first remaining one (or the settings
/// locale when the map has none left).
fn locale_delete_locale(node: &Gd<MindDialog>, name: &str) {
    let mut state = locale_state(node);
    let next = locale_remove_locale(&mut state, name);
    locale_set_state(node, &state);
    let selected = match next {
        Some(next) => next,
        None => {
            let stored = settings_text("locale", "");
            if stored.is_empty() || stored == "default" {
                locale_default_locale()
            } else {
                stored
            }
        }
    };
    set_meta_text(&mut node.clone(), LOCALE_SELECTED_META, &selected);
    locale_mark_dirty(node);
}

/// The `resized(this::buildMain)` equivalent: rebuild the cards when the
/// viewport width crosses a column boundary.
pub(super) fn locale_bind_resize(dialog: &mut MindDialog) {
    let node = dialog.to_gd();
    if node.has_meta(LOCALE_RESIZE_BOUND_META) {
        return;
    }
    let Some(mut viewport) = dialog.base().get_viewport() else {
        return;
    };
    let target = node.clone();
    let callable = Callable::from_fn("locales_resize", move |_args: &[&Variant]| {
        if !target.is_instance_valid() {
            return;
        }
        let columns = locale_main_columns(locale_viewport_width(&target));
        let previous = meta_int(&target, LOCALE_RESIZE_COLUMNS_META, -1);
        if previous == columns as i64 {
            return;
        }
        set_meta_int(
            &mut target.clone(),
            LOCALE_RESIZE_COLUMNS_META,
            columns as i64,
        );
        request_refresh(&target, LOCALE_REQUEST_MAIN);
    });
    viewport.connect("size_changed", &callable);
    node.clone()
        .set_meta(LOCALE_RESIZE_BOUND_META, &true.to_variant());
}

// -- sub-dialogs ------------------------------------------------------------

/// Opens a sub-dialog overlay (`new BaseDialog(...)` + `show()`).
///
/// The previous sub-dialog is detached immediately, not only queue-freed:
/// `begin_overlay` leaves the old layer in the tree for the rest of the frame,
/// so a same-name replacement would be renamed by Godot and later unfindable
/// by [`close_overlay`].
fn locale_overlay(
    node: &Gd<MindDialog>,
    name: &str,
    title: &str,
    width: f32,
    height: f32,
) -> Option<Overlay> {
    let mut dialog = node.clone().try_cast::<MindDialog>().ok()?;
    let mut bound = dialog.bind_mut();
    if let Some(mut existing) = bound.base().get_node_or_null(name) {
        if let Some(mut parent) = existing.get_parent() {
            parent.remove_child(&existing);
        }
        existing.queue_free();
    }
    begin_overlay(
        &mut bound,
        name,
        title,
        width,
        height,
        Color::from_rgba(0.0, 0.0, 0.0, 0.92),
    )
}

/// A `@back` button that frees the overlay (`dialog.addCloseButton()`).
fn locale_overlay_back(overlay: &Overlay) -> Gd<Button> {
    let mut back = factory::icon_button(
        "left",
        &kit::t("@back").to_string(),
        "defaultt",
        AlignmentMode::BEGIN,
    );
    back.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    let layer = overlay.layer.clone();
    let callable = Callable::from_fn("locales_overlay_back", move |_args: &[&Variant]| {
        if layer.is_instance_valid() {
            let mut layer = layer.clone();
            layer.queue_free();
        }
    });
    back.connect("pressed", &callable);
    back
}

/// `addLocaleDialog` (`MapLocalesDialog.java:350-374`): the available locales
/// not yet in the map.
pub(super) fn locale_show_add_locale(node: &Gd<MindDialog>) {
    let Some(overlay) = locale_overlay(
        node,
        LOCALE_SUB_OVERLAY,
        &kit::t("@add").to_string(),
        480.0,
        360.0,
    ) else {
        return;
    };
    let mut page = VBoxContainer::new_alloc();
    page.add_theme_constant_override("separation", 6);
    let state = locale_state(node);
    let selected = locale_selected(node);
    let mut any = false;
    for code in available_locales() {
        if state.0.contains_key(&code) {
            continue;
        }
        any = true;
        let mut button = factory::button(&locale_display_name(&code));
        button.set_theme_type_variation("flatTogglet");
        button.set_toggle_mode(true);
        button.set_pressed_no_signal(code == selected);
        button.set_custom_minimum_size(Vector2::new(400.0, 50.0));
        let target = node.clone();
        let layer = overlay.layer.clone();
        let name = code.clone();
        let callable = Callable::from_fn("locales_add_locale_pick", move |_args: &[&Variant]| {
            // Java keeps the `selectedLocale` guard even though the list only
            // holds absent bundles (`MapLocalesDialog.java:358`).
            if name == locale_selected(&target) {
                return;
            }
            let mut state = locale_state(&target);
            ensure_locale(&mut state, &name);
            locale_set_state(&target, &state);
            set_meta_text(&mut target.clone(), LOCALE_SELECTED_META, &name);
            locale_mark_dirty(&target);
            if layer.is_instance_valid() {
                let mut layer = layer.clone();
                layer.queue_free();
            }
            request_refresh(&target, LOCALE_REQUEST_TABLES);
        });
        button.connect("pressed", &callable);
        page.add_child(&button);
    }
    if !any {
        page.add_child(&as_control(&factory::label(&kit::t("@empty").to_string())));
    }
    page.add_child(&locale_overlay_back(&overlay));
    overlay.add_content(&page);
}

/// `localeEditDialog` (`MapLocalesDialog.java:425-449`): copy/load one locale's
/// `.properties` text.
pub(super) fn locale_show_locale_edit(node: &Gd<MindDialog>, locale: &str) {
    let Some(overlay) = locale_overlay(
        node,
        LOCALE_SUB_OVERLAY,
        &kit::t("@edit").to_string(),
        480.0,
        260.0,
    ) else {
        return;
    };
    let mut page = VBoxContainer::new_alloc();
    page.add_theme_constant_override("separation", 8);
    let mut frame = PanelContainer::new_alloc();
    frame.add_theme_stylebox_override("panel", &button_nine_patch("button"));
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 6);
    frame.add_child(&column);
    let copy_text = kit::t("@copy.clipboard").to_string();
    let mut copy = factory::icon_button("copy", &copy_text, "flatt", AlignmentMode::BEGIN);
    copy.set_custom_minimum_size(Vector2::new(350.0, 60.0));
    let target = node.clone();
    let layer = overlay.layer.clone();
    let name = locale.to_string();
    let callable = Callable::from_fn("locales_locale_copy", move |_args: &[&Variant]| {
        let state = locale_state(&target);
        let text = state.0.get(&name).map(write_locale).unwrap_or_default();
        DisplayServer::singleton().clipboard_set(&GString::from(text.as_str()));
        ui_show_info(&kit::t("@copied"));
        if layer.is_instance_valid() {
            let mut layer = layer.clone();
            layer.queue_free();
        }
    });
    copy.connect("pressed", &callable);
    column.add_child(&copy);
    let load_text = kit::t("@load.clipboard").to_string();
    let mut load = factory::icon_button("download", &load_text, "flatt", AlignmentMode::BEGIN);
    load.set_custom_minimum_size(Vector2::new(350.0, 60.0));
    let target = node.clone();
    let layer = overlay.layer.clone();
    let name = locale.to_string();
    let callable = Callable::from_fn("locales_locale_load", move |_args: &[&Variant]| {
        let text = DisplayServer::singleton().clipboard_get();
        let loaded = read_locale(&text.to_string());
        let mut state = locale_state(&target);
        state.0.insert(name.clone(), loaded);
        locale_set_state(&target, &state);
        locale_mark_dirty(&target);
        if layer.is_instance_valid() {
            let mut layer = layer.clone();
            layer.queue_free();
        }
        request_refresh(&target, LOCALE_REQUEST_TABLES);
    });
    load.connect("pressed", &callable);
    column.add_child(&load);
    page.add_child(&frame);
    page.add_child(&locale_overlay_back(&overlay));
    overlay.add_content(&page);
}

/// `editDialog` (`MapLocalesDialog.java:451-481`): copy/load every bundle and
/// roll the whole map back to the last apply.
pub(super) fn locale_show_edit_menu(node: &Gd<MindDialog>) {
    let Some(overlay) = locale_overlay(
        node,
        LOCALE_SUB_OVERLAY,
        &kit::t("@edit").to_string(),
        520.0,
        300.0,
    ) else {
        return;
    };
    let mut page = VBoxContainer::new_alloc();
    page.add_theme_constant_override("separation", 8);
    let mut frame = PanelContainer::new_alloc();
    frame.add_theme_stylebox_override("panel", &button_nine_patch("button"));
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 6);
    frame.add_child(&column);
    let copy_text = kit::t("@copy.clipboard").to_string();
    let mut copy = factory::icon_button("copy", &copy_text, "flatt", AlignmentMode::BEGIN);
    copy.set_custom_minimum_size(Vector2::new(450.0, 60.0));
    let target = node.clone();
    let layer = overlay.layer.clone();
    let callable = Callable::from_fn("locales_bundles_copy", move |_args: &[&Variant]| {
        let state = locale_state(&target);
        let text = locale_write_bundles(&state);
        DisplayServer::singleton().clipboard_set(&GString::from(text.as_str()));
        ui_show_info(&kit::t("@copied"));
        if layer.is_instance_valid() {
            let mut layer = layer.clone();
            layer.queue_free();
        }
    });
    copy.connect("pressed", &callable);
    column.add_child(&copy);
    let load_text = kit::t("@load.clipboard").to_string();
    let mut load = factory::icon_button("download", &load_text, "flatt", AlignmentMode::BEGIN);
    load.set_custom_minimum_size(Vector2::new(450.0, 60.0));
    let target = node.clone();
    let layer = overlay.layer.clone();
    let callable = Callable::from_fn("locales_bundles_load", move |_args: &[&Variant]| {
        let text = DisplayServer::singleton().clipboard_get();
        let loaded = locale_read_bundles(&text.to_string());
        locale_set_state(&target, &loaded);
        locale_mark_dirty(&target);
        if layer.is_instance_valid() {
            let mut layer = layer.clone();
            layer.queue_free();
        }
        request_refresh(&target, LOCALE_REQUEST_TABLES);
    });
    load.connect("pressed", &callable);
    column.add_child(&load);
    let rollback_text = kit::t("@locales.rollback").to_string();
    let mut rollback = factory::icon_button("undo", &rollback_text, "flatt", AlignmentMode::BEGIN);
    rollback.set_custom_minimum_size(Vector2::new(450.0, 60.0));
    rollback.set_disabled(meta_bool(node, LOCALE_SAVED_META));
    let target = node.clone();
    let layer = overlay.layer.clone();
    let callable = Callable::from_fn("locales_bundles_rollback", move |_args: &[&Variant]| {
        let saved = locale_saved_state(&target);
        locale_set_state(&target, &saved);
        set_meta_bool(&mut target.clone(), LOCALE_SAVED_META, true);
        locale_sync_apply(&target, true);
        if layer.is_instance_valid() {
            let mut layer = layer.clone();
            layer.queue_free();
        }
        request_refresh(&target, LOCALE_REQUEST_TABLES);
    });
    rollback.connect("pressed", &callable);
    column.add_child(&rollback);
    page.add_child(&frame);
    page.add_child(&locale_overlay_back(&overlay));
    overlay.add_content(&page);
}

/// `propEditDialog` (`MapLocalesDialog.java:376-423`): per-property actions.
fn locale_show_prop_edit(node: &Gd<MindDialog>, key: &str, value: &str) {
    let Some(overlay) = locale_overlay(
        node,
        LOCALE_SUB_OVERLAY,
        &kit::t("@edit").to_string(),
        520.0,
        330.0,
    ) else {
        return;
    };
    let mut page = VBoxContainer::new_alloc();
    page.add_theme_constant_override("separation", 8);
    let mut frame = PanelContainer::new_alloc();
    frame.add_theme_stylebox_override("panel", &button_nine_patch("button"));
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 6);
    frame.add_child(&column);

    let add_other_text = kit::t("@locales.addtoother").to_string();
    let mut add_other = factory::icon_button("add", &add_other_text, "flatt", AlignmentMode::BEGIN);
    add_other.set_custom_minimum_size(Vector2::new(450.0, 60.0));
    let target = node.clone();
    let layer = overlay.layer.clone();
    let property = key.to_string();
    let property_value = value.to_string();
    let callable = Callable::from_fn("locales_prop_add_other", move |_args: &[&Variant]| {
        let mut state = locale_state(&target);
        locale_add_to_other(&mut state, &property, &property_value);
        locale_set_state(&target, &state);
        locale_mark_dirty(&target);
        if layer.is_instance_valid() {
            let mut layer = layer.clone();
            layer.queue_free();
        }
        request_refresh(&target, LOCALE_REQUEST_MAIN);
    });
    add_other.connect("pressed", &callable);
    column.add_child(&add_other);

    let view_text = kit::t("@locales.viewproperty").to_string();
    let mut view = factory::icon_button("zoom", &view_text, "flatt", AlignmentMode::BEGIN);
    view.set_custom_minimum_size(Vector2::new(450.0, 60.0));
    let target = node.clone();
    let property = key.to_string();
    let callable = Callable::from_fn("locales_prop_view", move |_args: &[&Variant]| {
        locale_show_prop_view(&target, &property);
    });
    view.connect("pressed", &callable);
    column.add_child(&view);

    let icon_text = kit::t("@locales.addicon").to_string();
    let mut add_icon = factory::icon_button("image", &icon_text, "flatt", AlignmentMode::BEGIN);
    add_icon.set_custom_minimum_size(Vector2::new(450.0, 60.0));
    let target = node.clone();
    let property = key.to_string();
    let property_value = value.to_string();
    let callable = Callable::from_fn("locales_prop_add_icon", move |_args: &[&Variant]| {
        locale_show_add_icon(&target, &property, &property_value);
    });
    add_icon.connect("pressed", &callable);
    column.add_child(&add_icon);

    let rollback_text = kit::t("@locales.rollback").to_string();
    let mut rollback = factory::icon_button("undo", &rollback_text, "flatt", AlignmentMode::BEGIN);
    rollback.set_custom_minimum_size(Vector2::new(450.0, 60.0));
    rollback.set_disabled(!locale_rollback_available(node, key));
    let target = node.clone();
    let layer = overlay.layer.clone();
    let property = key.to_string();
    let callable = Callable::from_fn("locales_prop_rollback", move |_args: &[&Variant]| {
        let selected = locale_selected(&target);
        let saved = locale_saved_state(&target);
        if let Some(previous) = saved
            .0
            .get(&selected)
            .and_then(|props| props.get(&property))
            .cloned()
        {
            let mut state = locale_state(&target);
            if let Some(props) = state.0.get_mut(&selected) {
                props.insert(property.clone(), previous);
            }
            locale_set_state(&target, &state);
        }
        if layer.is_instance_valid() {
            let mut layer = layer.clone();
            layer.queue_free();
        }
        request_refresh(&target, LOCALE_REQUEST_TABLES);
    });
    rollback.connect("pressed", &callable);
    column.add_child(&rollback);

    page.add_child(&frame);
    page.add_child(&locale_overlay_back(&overlay));
    overlay.add_content(&page);
}

/// `viewPropertyDialog` (`MapLocalesDialog.java:483-509`): one property across
/// every locale, with its own search and the shared filter dialog.
fn locale_show_prop_view(node: &Gd<MindDialog>, key: &str) {
    let title = kit::tm_format("@locales.viewing", &[key]).to_string();
    let Some(overlay) = locale_overlay(node, LOCALE_SUB_OVERLAY, &title, 980.0, 400.0) else {
        return;
    };
    let mut grid = factory::table();
    grid.bind_mut().grow_horizontal();
    let mut search_row = HBoxContainer::new_alloc();
    search_row.add_theme_constant_override("separation", 6);
    let grow_target = node.clone();
    let grow_grid = grid.clone();
    let grow_key = key.to_string();
    let on_close = Callable::from_fn("locales_prop_filter_close", move |_args: &[&Variant]| {
        locale_fill_prop_view(&grow_grid, &grow_target, &grow_key);
    });
    let filter_target = node.clone();
    let callable = Callable::from_fn("locales_prop_filter", move |_args: &[&Variant]| {
        locale_show_filter(&filter_target, on_close.clone());
    });
    let mut filter = factory::glyph_button("filter", "", "emptyi");
    filter.set_custom_minimum_size(Vector2::new(35.0, 35.0));
    filter.connect("pressed", &callable);
    search_row.add_child(&filter);
    let by_value = meta_bool(node, LOCALE_SEARCH_VALUE_META);
    let placeholder = if by_value {
        kit::t("@locales.searchvalue")
    } else {
        kit::t("@locales.searchlocale")
    };
    let mut search = factory::field(&placeholder.to_string());
    search.set_max_length(64);
    search.set_custom_minimum_size(Vector2::new(250.0, 0.0));
    search.set_text(&GString::from(meta_text(node, LOCALE_SEARCH_META).as_str()));
    let search_target = node.clone();
    let search_grid = grid.clone();
    let search_key = key.to_string();
    let callable = Callable::from_fn("locales_prop_search", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|entry| entry.to::<GString>().to_string())
            .unwrap_or_default();
        set_meta_text(&mut search_target.clone(), LOCALE_SEARCH_META, &text);
        locale_fill_prop_view(&search_grid, &search_target, &search_key);
    });
    search.connect("text_changed", &callable);
    search_row.add_child(&search);
    let clear_target = node.clone();
    let clear_grid = grid.clone();
    let clear_key = key.to_string();
    let clear_field = search.clone();
    let callable = Callable::from_fn("locales_prop_search_clear", move |_args: &[&Variant]| {
        set_meta_text(&mut clear_target.clone(), LOCALE_SEARCH_META, "");
        let mut field = clear_field.clone();
        field.set_text(&GString::from(""));
        locale_fill_prop_view(&clear_grid, &clear_target, &clear_key);
    });
    let mut clear = factory::glyph_button("cancel", "", "emptyi");
    clear.set_custom_minimum_size(Vector2::new(35.0, 35.0));
    clear.connect("pressed", &callable);
    search_row.add_child(&clear);

    // Insert the search row between the overlay title and its scroll pane, so
    // the cards scroll while the search row stays put
    // (`dialog.cont.table(...).row(); dialog.cont.add(propView)`).
    let search_control = as_control(&search_row);
    let mut frame = overlay.frame_box.clone();
    let index = (frame.get_child_count() - 1).max(0);
    frame.add_child(&search_control);
    frame.move_child(&search_control, index);
    overlay.add_content(&grid);
    let mut back = factory::icon_button(
        "left",
        &kit::t("@back").to_string(),
        "defaultt",
        AlignmentMode::BEGIN,
    );
    back.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    let layer = overlay.layer.clone();
    let back_target = node.clone();
    let callable = Callable::from_fn("locales_prop_view_back", move |_args: &[&Variant]| {
        if layer.is_instance_valid() {
            let mut layer = layer.clone();
            layer.queue_free();
        }
        request_refresh(&back_target, LOCALE_REQUEST_MAIN);
    });
    back.connect("pressed", &callable);
    frame.add_child(&as_control(&back));
    locale_fill_prop_view(&grid, node, key);
}

/// `buildPropView` (`MapLocalesDialog.java:511-578`): the locale cards for
/// `key`. Missing locales show the `@add` button that plants
/// [`LOCALE_PLACEHOLDER_VALUE`] and turns into a value area.
fn locale_fill_prop_view(grid: &Gd<MindTable>, node: &Gd<MindDialog>, key: &str) {
    let grid_handle = grid.clone();
    let mut grid = grid.clone();
    let mut bound = grid.bind_mut();
    bound.clear();
    let state = locale_state(node);
    let search = meta_text(node, LOCALE_SEARCH_META);
    let by_value = meta_bool(node, LOCALE_SEARCH_VALUE_META);
    let (show_correct, show_missing, show_same) = locale_show_flags(node);
    let columns = locale_view_columns(locale_viewport_width(node));
    let mut shown = 0usize;
    for code in available_locales() {
        let Some(props) = state.0.get(&code) else {
            continue;
        };
        let value = props.get(key).cloned();
        let status = locale_property_status(&state, key, value.as_deref(), &code, true);
        if !locale_status_visible(status, show_correct, show_missing, show_same) {
            continue;
        }
        if status != LocaleStatus::Missing {
            let candidate = if by_value {
                value.clone().unwrap_or_default()
            } else {
                locale_display_name(&code)
            };
            if !locale_matches_search(&search, &candidate) {
                continue;
            }
        }
        let card = locale_build_prop_card(node, &grid_handle, key, &code, value.as_deref(), status);
        {
            let bound_card = bound.add(&card).width(LOCALE_CARD_WIDTH).pad(5.0);
            let _ = bound_card.top();
        }
        shown += 1;
        if shown.is_multiple_of(columns) {
            bound.row();
        }
    }
    if shown == 0 {
        let label = factory::label(&kit::t("@empty").to_string());
        bound.add(&label).colspan(columns as i32).pad(20.0);
        bound.row();
    }
}

/// One locale card of the property view (`colTables[i].table(Tex.whitePane,
/// t -> ...)`, `MapLocalesDialog.java:540-575`).
fn locale_build_prop_card(
    node: &Gd<MindDialog>,
    grid: &Gd<MindTable>,
    key: &str,
    locale: &str,
    value: Option<&str>,
    status: LocaleStatus,
) -> Gd<Control> {
    // code-instantiated: one card per locale row of the property view.
    let mut card = PanelContainer::new_alloc();
    card.add_theme_stylebox_override("panel", &locale_card_frame(locale_status_color(status)));
    card.set_custom_minimum_size(Vector2::new(LOCALE_CARD_WIDTH, 0.0));
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 4);
    card.add_child(&column);
    let mut title = factory::label(&locale_display_name(locale));
    title.add_theme_color_override("font_color", styles::ACCENT);
    column.add_child(&as_control(&title));
    column.add_child(&locale_rule(styles::ACCENT));
    if status == LocaleStatus::Missing {
        let add_text = kit::t("@add").to_string();
        let mut add = factory::icon_button("add", &add_text, "defaultt", AlignmentMode::CENTER);
        add.set_custom_minimum_size(Vector2::new(160.0, 50.0));
        add.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        let target = node.clone();
        let grid = grid.clone();
        let property = key.to_string();
        let name = locale.to_string();
        let callable = Callable::from_fn("locales_prop_add", move |_args: &[&Variant]| {
            let mut state = locale_state(&target);
            if let Some(props) = state.0.get_mut(&name) {
                props.insert(property.clone(), String::from(LOCALE_PLACEHOLDER_VALUE));
            }
            locale_set_state(&target, &state);
            locale_mark_dirty(&target);
            locale_fill_prop_view(&grid, &target, &property);
        });
        add.connect("pressed", &callable);
        column.add_child(&add);
    } else {
        let mut area = text_area(value.unwrap_or(""));
        area.set_custom_minimum_size(Vector2::new(0.0, 140.0));
        area.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let target = node.clone();
        let card_for_value = card.clone();
        let area_for_value = area.clone();
        let property = key.to_string();
        let name = locale.to_string();
        let callable = Callable::from_fn("locales_prop_value", move |_args: &[&Variant]| {
            let text = area_for_value.get_text().to_string();
            let mut state = locale_state(&target);
            if let Some(props) = state.0.get_mut(&name) {
                props.insert(property.clone(), text.clone());
            }
            locale_set_state(&target, &state);
            locale_mark_dirty(&target);
            let status = locale_property_status(&state, &property, Some(&text), &name, true);
            locale_update_card(&card_for_value, locale_status_color(status));
        });
        area.connect("text_changed", &callable);
        column.add_child(&area);
    }
    card.upcast()
}

/// `addIconDialog` (`MapLocalesDialog.java:619-642`): the icon/content/team
/// gallery; picking one appends its emoji to the property value.
fn locale_show_add_icon(node: &Gd<MindDialog>, key: &str, value: &str) {
    let Some(overlay) = locale_overlay(
        node,
        LOCALE_SUB_OVERLAY,
        &kit::t("@locales.addicon").to_string(),
        900.0,
        420.0,
    ) else {
        return;
    };
    let mut search_row = HBoxContainer::new_alloc();
    search_row.add_theme_constant_override("separation", 6);
    let mut search = factory::field(&kit::t("@search").to_string());
    search.set_max_length(64);
    search.set_custom_minimum_size(Vector2::new(250.0, 0.0));
    search_row.add_child(&search);
    let mut grid = factory::table();
    grid.bind_mut().grow_horizontal();
    let layer = overlay.layer.clone();
    let grid_for_search = grid.clone();
    let target = node.clone();
    let layer_for_search = layer.clone();
    let key_for_search = key.to_string();
    let value_for_search = value.to_string();
    let search_field = search.clone();
    let callable = Callable::from_fn("locales_icon_search", move |_args: &[&Variant]| {
        let text = search_field
            .get_text()
            .to_string()
            .replace(' ', "")
            .to_lowercase();
        locale_fill_icon_grid(
            &grid_for_search,
            &target,
            &layer_for_search,
            &key_for_search,
            &value_for_search,
            &text,
        );
    });
    search.connect("text_changed", &callable);
    let clear_field = search.clone();
    let clear_grid = grid.clone();
    let clear_target = node.clone();
    let clear_layer = layer.clone();
    let clear_key = key.to_string();
    let clear_value = value.to_string();
    let callable = Callable::from_fn("locales_icon_search_clear", move |_args: &[&Variant]| {
        let mut field = clear_field.clone();
        field.set_text(&GString::from(""));
        locale_fill_icon_grid(
            &clear_grid,
            &clear_target,
            &clear_layer,
            &clear_key,
            &clear_value,
            "",
        );
    });
    let mut clear = factory::glyph_button("cancel", "", "emptyi");
    clear.set_custom_minimum_size(Vector2::new(35.0, 35.0));
    clear.connect("pressed", &callable);
    search_row.add_child(&clear);
    let search_control = as_control(&search_row);
    let mut frame = overlay.frame_box.clone();
    let index = (frame.get_child_count() - 1).max(0);
    frame.add_child(&search_control);
    frame.move_child(&search_control, index);
    overlay.add_content(&grid);
    locale_fill_icon_grid(&grid, node, &layer, key, value, "");
}

/// `filterDialog` (`MapLocalesDialog.java:580-617`): the search-by-name/value
/// choice and the three status filters. `on_close` mirrors the Java
/// `Runnable hidden` (rebuild the caller's pane when the dialog closes).
pub(super) fn locale_show_filter(node: &Gd<MindDialog>, on_close: Callable) {
    let Some(overlay) = locale_overlay(
        node,
        LOCALE_FILTER_OVERLAY,
        &kit::t("@locales.filter").to_string(),
        520.0,
        420.0,
    ) else {
        return;
    };
    let mut page = VBoxContainer::new_alloc();
    page.add_theme_constant_override("separation", 10);
    page.add_child(&as_control(&factory::label(&kit::t("@search").to_string())));
    let by_value = meta_bool(node, LOCALE_SEARCH_VALUE_META);
    let mut by_name = locale_mode_button("@locales.byname", !by_value);
    let mut by_value_button = locale_mode_button("@locales.byvalue", by_value);
    let mut mode_row = HBoxContainer::new_alloc();
    mode_row.add_theme_constant_override("separation", 10);
    let name_target = node.clone();
    let name_button = by_name.clone();
    let value_button = by_value_button.clone();
    let callable = Callable::from_fn("locales_search_by_name", move |_args: &[&Variant]| {
        set_meta_bool(&mut name_target.clone(), LOCALE_SEARCH_VALUE_META, false);
        let mut name = name_button.clone();
        name.set_pressed_no_signal(true);
        let mut value = value_button.clone();
        value.set_pressed_no_signal(false);
    });
    by_name.connect("pressed", &callable);
    let value_target = node.clone();
    let name_button = by_name.clone();
    let value_button = by_value_button.clone();
    let callable = Callable::from_fn("locales_search_by_value", move |_args: &[&Variant]| {
        set_meta_bool(&mut value_target.clone(), LOCALE_SEARCH_VALUE_META, true);
        let mut name = name_button.clone();
        name.set_pressed_no_signal(false);
        let mut value = value_button.clone();
        value.set_pressed_no_signal(true);
    });
    by_value_button.connect("pressed", &callable);
    mode_row.add_child(&by_name);
    mode_row.add_child(&by_value_button);
    page.add_child(&mode_row);
    let filters = [
        (
            LOCALE_SHOW_CORRECT_META,
            "@locales.showcorrect",
            LOCALE_GRAY,
            meta_bool(node, LOCALE_SHOW_CORRECT_META),
        ),
        (
            LOCALE_SHOW_MISSING_META,
            "@locales.showmissing",
            styles::ACCENT,
            meta_bool(node, LOCALE_SHOW_MISSING_META),
        ),
        (
            LOCALE_SHOW_SAME_META,
            "@locales.showsame",
            LOCALE_TECH_BLUE,
            meta_bool(node, LOCALE_SHOW_SAME_META),
        ),
    ];
    for (meta, key, color, checked) in filters {
        let (mut button, mut glyph) =
            locale_filter_toggle(&kit::t(key).to_string(), color, checked);
        let state = button.clone();
        let target = node.clone();
        let callable = Callable::from_fn("locales_filter_toggle", move |_args: &[&Variant]| {
            let checked = state.is_pressed();
            set_meta_bool(&mut target.clone(), meta, checked);
            glyph.set_text(&kit::glyph(if checked { "ok" } else { "cancel" }));
        });
        button.connect("pressed", &callable);
        page.add_child(&button);
    }
    let mut back = factory::icon_button(
        "left",
        &kit::t("@back").to_string(),
        "defaultt",
        AlignmentMode::BEGIN,
    );
    back.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    let layer = overlay.layer.clone();
    let callable = Callable::from_fn("locales_filter_back", move |_args: &[&Variant]| {
        if layer.is_instance_valid() {
            let mut layer = layer.clone();
            layer.queue_free();
        }
        let _ = on_close.callv(&VarArray::new());
    });
    back.connect("pressed", &callable);
    page.add_child(&back);
    overlay.add_content(&page);
}

/// `Styles.togglet` search-mode button (`filterDialog`,
/// `MapLocalesDialog.java:586-591`).
fn locale_mode_button(key: &str, pressed: bool) -> Gd<Button> {
    let mut button = factory::button(&kit::t(key).to_string());
    button.set_theme_type_variation("togglet");
    button.set_toggle_mode(true);
    button.set_pressed_no_signal(pressed);
    button.set_custom_minimum_size(Vector2::new(300.0, 50.0));
    button
}
