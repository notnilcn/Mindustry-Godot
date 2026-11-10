// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The mods dialog's list pane: the rebuild, its row and the state markup.
//! Upstream: ui/dialogs/ModsDialog.java

use super::mods_detail::{
    mods_add_details, mods_confirm_delete, mods_set_enabled, mods_toggle_details,
};
use super::mods_dialog::{ModsState, ROW_HEIGHT};
use super::mods_import::{mods_show_import_dialog, mods_update_mod};
use super::util::{
    apply_pane_button, bm, bt, build_button_content, defer_action, dict_bool, dict_string,
    flat_style, fmt_key, mods_node, open_dialog, string_list, variant_string,
};
use crate::ui::dialog::MindDialog;
use crate::ui::kit::widgets::border_image::MindBorderImage;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindLabel, styles};
use godot::builtin::Side;
use godot::classes::control::{FocusMode, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, ColorRect, Control, HBoxContainer, Node, StyleBoxEmpty, VBoxContainer,
};
use godot::prelude::*;
use mind_core::util::strip_colors;
use std::{cell::RefCell, rc::Rc};

/// One `MindMods.list()` row set (`[]` when the autoload is absent).
pub(super) fn mods_list(mods: &Option<Gd<Node>>) -> Vec<VarDictionary> {
    let Some(mut mods) = mods.clone() else {
        return Vec::new();
    };
    if !mods.has_method("list") {
        return Vec::new();
    }
    let value = mods.call("list", &[]);
    let Ok(rows) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    rows.iter_shared()
        .filter_map(|row| row.try_to::<VarDictionary>().ok())
        .collect()
}

/// One `MindMods.details(name)` row (`{}` when absent).
pub(super) fn mods_details(name: &str) -> VarDictionary {
    let Some(mut mods) = mods_node() else {
        return VarDictionary::new();
    };
    if !mods.has_method("details") {
        return VarDictionary::new();
    }
    mods.call("details", &[GString::from(name).to_variant()])
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// `MindMods.errors()` rows.
pub(super) fn mods_errors(mods: &Option<Gd<Node>>) -> Vec<VarDictionary> {
    let Some(mut mods) = mods.clone() else {
        return Vec::new();
    };
    if !mods.has_method("errors") {
        return Vec::new();
    }
    let value = mods.call("errors", &[]);
    let Ok(rows) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    rows.iter_shared()
        .filter_map(|row| row.try_to::<VarDictionary>().ok())
        .collect()
}

/// `MindMods.requires_reload()`.
pub(super) fn mods_requires_reload() -> bool {
    let Some(mut mods) = mods_node() else {
        return false;
    };
    if !mods.has_method("requires_reload") {
        return false;
    }
    mods.call("requires_reload", &[]).to::<bool>()
}

/// `cont.table(buttons -> …)` (`:139-201`): `@mod.import` (Icon.add) and
/// `@mods.browser` (Icon.menu), `Styles.flatBordert`, `growX().height(60f)
/// .uniformX()`, `margin(12f)`.
pub(super) fn mods_build_action_row() -> Gd<HBoxContainer> {
    // code-instantiated: the import/browser pair is rebuilt with `cont`.
    let mut actions = HBoxContainer::new_alloc();
    actions.add_theme_constant_override("separation", 0);
    {
        let import_icon = factory::glyph_label("add", 24, Color::WHITE);
        let mut import_button =
            mods_flat_bordered_button("@mod.import", Some(import_icon.upcast()));
        let callable = Callable::from_fn("mods_show_import", move |_args: &[&Variant]| {
            defer_action("mods_show_import_now", mods_show_import_dialog);
            Variant::nil()
        });
        import_button.connect("pressed", &callable);
        actions.add_child(&import_button);
    }
    {
        let browser_icon = factory::glyph_label("menu", 24, Color::WHITE);
        let mut browser_button =
            mods_flat_bordered_button("@mods.browser", Some(browser_icon.upcast()));
        let callable = Callable::from_fn("mods_open_browser", move |_args: &[&Variant]| {
            defer_action("mods_open_browser_now", || open_dialog("mod_browser", "{}"));
            Variant::nil()
        });
        browser_button.connect("pressed", &callable);
        actions.add_child(&browser_button);
    }
    actions
}

/// `Styles.flatBordert` (`Styles.java:174-181`): `up = pane`, `over =
/// flatDownBase` (the white-framed 9-patch), `down = flatOver`.
pub(super) fn mods_flat_bordered_button(key: &str, icon: Option<Gd<Control>>) -> Gd<Button> {
    // code-instantiated: action buttons are rebuilt with `cont`.
    let mut button = factory::button("");
    button.set_h_size_flags(SizeFlags::EXPAND_FILL);
    button.set_focus_mode(FocusMode::NONE);
    apply_pane_button(&mut button, styles::PANE_OUTLINE_GRAY, 4.0);
    button.add_theme_stylebox_override(
        "hover",
        &MindDialog::pane_box(Color::BLACK, Color::WHITE, 4.0, 2.0, -1.0),
    );
    button.add_theme_stylebox_override(
        "pressed",
        &MindDialog::pane_box(styles::FLAT_OVER, styles::FLAT_OVER, 4.0, 2.0, -1.0),
    );
    build_button_content(&mut button, icon, &bt(key));
    button
}

/// `rebuild.get(query)` (`:209-314`): one `Styles.grayt` card per matching mod,
/// a gray rule before the first disabled one.
pub(super) fn mods_rebuild_list(state: &Rc<RefCell<ModsState>>, st: &mut ModsState, query: &str) {
    let Some(mut list) = st.list.clone() else {
        return;
    };
    if !list.is_instance_valid() {
        return;
    }
    list.bind_mut().clear();
    let entries = mods_list(&mods_node());
    let needle = query.trim().to_lowercase();
    let mut any = false;
    for entry in &entries {
        let display = strip_colors(&mods_display_name(entry));
        // `Strings.matches(query, displayName)`: case-insensitive contains.
        if !needle.is_empty() && !display.to_lowercase().contains(&needle) {
            continue;
        }
        any = true;
        if !dict_bool(entry, "enabled", false) && !st.any_disabled {
            st.any_disabled = true;
            // `cont.image().growX().height(4f).pad(6f).color(Pal.gray)`.
            // code-instantiated: the separator precedes the first disabled row.
            let mut rule = ColorRect::new_alloc();
            rule.set_color(styles::FLAT_OVER);
            rule.set_custom_minimum_size(Vector2::new(0.0, 4.0));
            let mut bound = list.bind_mut();
            bound.add(&rule).grow_x().height(4.0).pad(6.0);
            bound.row();
        }
        mods_add_mod_row(state, st, entry);
    }
    if !any {
        let mut bound = list.bind_mut();
        bound.add(&factory::label(&bm("@none.found"))).pad(4.0);
        bound.row();
    }
    list.bind_mut().sort_now();
    mods_defer_fit(state);
}

/// The pane takes its content height, capped to the space left between the
/// title and the button bar (an Arc `ScrollPane` cell shrinks to fit).
pub(super) fn mods_fit_pane(state: &mut ModsState) {
    let (Some(mut pane), Some(list), Some(root), Some(body)) = (
        state.pane.clone(),
        state.list.clone(),
        state.root.clone(),
        state.body.clone(),
    ) else {
        return;
    };
    if !pane.is_instance_valid()
        || !list.is_instance_valid()
        || !root.is_instance_valid()
        || !body.is_instance_valid()
    {
        return;
    }
    let content_height = list.get_combined_minimum_size().y;
    let others = root.get_combined_minimum_size().y - pane.get_custom_minimum_size().y;
    let available = (body.get_size().y - others).max(ROW_HEIGHT);
    let target = content_height.min(available);
    if (pane.get_custom_minimum_size().y - target).abs() > 0.0005 {
        pane.set_custom_minimum_size(Vector2::new(0.0, target));
        root.clone().queue_sort();
    }
}

/// Queues one [`mods_fit_pane`] pass after the current callback.
pub(super) fn mods_defer_fit(state: &Rc<RefCell<ModsState>>) {
    let slot = state.clone();
    defer_action("mods_fit_pane", move || {
        if let Ok(mut st) = slot.try_borrow_mut() {
            mods_fit_pane(&mut st);
        }
    });
}

/// `getStateText` (`:356-375`): the state line under a mod's name.
pub(super) fn mods_state_text(entry: &VarDictionary) -> String {
    match dict_string(entry, "state").as_str() {
        "incompatible" => bt("@mod.blacklisted"),
        // Port-only state (no JS engine, OD1): the row shows the reason text
        // (`UnsupportedReason::message`, e.g. scripts) that `list()` carries.
        "unsupported" => {
            let reason = dict_string(entry, "reason");
            if reason.is_empty() {
                bt("@mod.blacklisted")
            } else {
                reason
            }
        }
        "circularDependencies" => bt("@mod.circulardependencies"),
        "incompleteDependencies" => bt("@mod.incompletedependencies"),
        "missingDependencies" => bt("@mod.unmetdependencies"),
        "contentErrors" => bt("@mod.erroredcontent"),
        _ => {
            if dict_bool(entry, "hidden", false) {
                bt("@mod.multiplayer.compatible")
            } else {
                String::new()
            }
        }
    }
}

/// `getStateDetails`: the tooltip/details body for a state.
pub(super) fn mods_state_details(entry: &VarDictionary) -> String {
    let missing = string_list(entry.get("missing_dependencies").as_ref()).join(", ");
    match dict_string(entry, "state").as_str() {
        "incompatible" | "unsupported" => {
            let reason = dict_string(entry, "reason");
            if reason.is_empty() {
                bt("@mod.blacklisted.details")
            } else {
                reason
            }
        }
        "circularDependencies" => bt("@mod.circulardependencies.details"),
        "incompleteDependencies" => {
            fmt_key("@mod.incompletedependencies.details", &[missing.as_str()])
        }
        "missingDependencies" => fmt_key("@mod.missingdependencies.details", &[missing.as_str()]),
        "contentErrors" => bt("@mod.erroredcontent.details"),
        _ => String::new(),
    }
}

/// `hideDisabled = !isSupported() || hasUnmetDependencies() || hasContentErrors()`.
pub(super) fn mods_hide_disabled(entry: &VarDictionary) -> bool {
    matches!(
        dict_string(entry, "state").as_str(),
        "unsupported"
            | "incompatible"
            | "missingDependencies"
            | "incompleteDependencies"
            | "circularDependencies"
            | "contentErrors"
    )
}

/// The card's two-line title (`:256-258`): `[accent]name` then the short
/// description when enabled (or `hideDisabled`), else `@mod.failed` /
/// `@mod.disabled`.
pub(super) fn mods_title_markup(entry: &VarDictionary, enabled: bool) -> String {
    let mut text = format!("[accent]{}\n", strip_colors(&mods_display_name(entry)));
    if enabled || mods_hide_disabled(entry) {
        let description = dict_string(entry, "short_description");
        if !description.is_empty() {
            text.push_str("[lightgray]");
            text.push_str(&description);
        }
    } else if dict_bool(entry, "failed", false) {
        text.push_str(&bt("@mod.failed"));
    } else {
        text.push_str(&bt("@mod.disabled"));
    }
    text
}

/// `_display_name(entry)`.
pub(super) fn mods_display_name(entry: &VarDictionary) -> String {
    for key in ["display_name", "displayName", "name"] {
        if let Some(value) = entry.get(key) {
            let text = variant_string(&value);
            if !text.is_empty() {
                return text;
            }
        }
    }
    String::new()
}

/// One `cont.button(t -> …, Styles.grayt, () -> showMod(mod)).size(w, h)
/// .growX().pad(4f).padTop(8f)` card (`:222-296`).
pub(super) fn mods_add_mod_row(
    state: &Rc<RefCell<ModsState>>,
    st: &mut ModsState,
    entry: &VarDictionary,
) {
    let Some(mut list) = st.list.clone() else {
        return;
    };
    let mod_name = dict_string(entry, "name");
    let enabled = dict_bool(entry, "enabled", false);
    let mod_state = dict_string(entry, "state");
    // `.disabled(!mod.isSupported())` on the toggle.
    let supported = mod_state != "unsupported" && mod_state != "incompatible";

    // code-instantiated: mod rows are data-driven from `MindMods.list()`.
    let mut card = mods_gray_button();
    // `t.addListener(new Tooltip(… add(stateDetails) …))` (plain-text tooltip).
    card.set_tooltip_text(&strip_colors(&mods_state_details(entry)));
    {
        let slot = state.clone();
        let name = mod_name.clone();
        let callable = Callable::from_fn("mods_toggle_details", move |_args: &[&Variant]| {
            if let Ok(mut st) = slot.try_borrow_mut() {
                mods_toggle_details(&slot, &mut st, &name);
            }
            Variant::nil()
        });
        card.connect("pressed", &callable);
    }
    // code-instantiated: the card's content row (`t.margin(12f)`, top-left).
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, 4.0);
    row.set_offset(Side::TOP, 4.0);
    row.set_offset(Side::RIGHT, -4.0);
    row.set_offset(Side::BOTTOM, -4.0);
    row.add_theme_constant_override("separation", 8);
    row.set_mouse_filter(MouseFilter::IGNORE);
    card.add_child(&row);

    // `title1.add(new BorderImage()).size(h - 8f)…b.setDrawable(Tex.nomap);
    // b.border(Pal.accent)`. The port has no per-mod icon texture yet
    // (`MindMods.list()` carries none), so every card shows `nomap`.
    // code-instantiated: per-row icon frame.
    let mut icon = MindBorderImage::new_alloc();
    if let Some(texture) = kit::icon_texture("nomap") {
        icon.set_texture(&texture);
    }
    icon.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    icon.set_stretch_mode(godot::classes::texture_rect::StretchMode::SCALE);
    icon.set_custom_minimum_size(Vector2::new(ROW_HEIGHT - 8.0, ROW_HEIGHT - 8.0));
    {
        let mut bound = icon.bind_mut();
        bound.thickness = 4.0;
        bound.border_color = styles::ACCENT;
    }
    icon.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&icon);

    // code-instantiated: per-row title/state column (`title1.table(text -> …)`).
    let mut text_column = VBoxContainer::new_alloc();
    text_column.set_h_size_flags(SizeFlags::EXPAND_FILL);
    text_column.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&text_column);
    // `.ellipsis(true).top().width(300f).growX()`: two clipped lines.
    let title_markup = kit::markup_text(&mods_title_markup(entry, enabled)).to_string();
    let mut title = factory::label(&title_markup);
    title.set_fit_content(false);
    title.set_clip_contents(true);
    title.set_custom_minimum_size(Vector2::new(300.0, 52.0));
    title.set_h_size_flags(SizeFlags::EXPAND_FILL);
    title.set_mouse_filter(MouseFilter::IGNORE);
    text_column.add_child(&title);
    let state_text = mods_state_text(entry);
    if !state_text.is_empty() {
        // `text.labelWrap(state).growX()`.
        let wrapped = kit::markup_text(&state_text).to_string();
        let mut state_label = factory::label(&wrapped);
        state_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        state_label.set_custom_minimum_size(Vector2::new(300.0, 0.0));
        state_label.set_h_size_flags(SizeFlags::EXPAND_FILL);
        state_label.set_mouse_filter(MouseFilter::IGNORE);
        text_column.add_child(&state_label);
    }

    // `t.table(right -> …)`: enable toggle + delete, `Styles.clearNonei` 50x50.
    // code-instantiated: per-row action column.
    let mut right = HBoxContainer::new_alloc();
    right.set_v_size_flags(SizeFlags::SHRINK_BEGIN);
    right.add_theme_constant_override("separation", 0);
    row.add_child(&right);
    // code-instantiated: the enable toggle (`Icon.downOpen`/`upOpen`).
    let fallback = if enabled {
        bt("@mod.enabled")
    } else {
        bt("@mod.disabled")
    };
    let mut toggle = factory::glyph_button(
        if enabled { "down-open" } else { "up-open" },
        &fallback,
        "clearNonei",
    );
    toggle.set_custom_minimum_size(Vector2::new(50.0, 50.0));
    toggle.set_focus_mode(FocusMode::NONE);
    toggle.set_disabled(!supported);
    {
        let slot = state.clone();
        let name = mod_name.clone();
        let callable = Callable::from_fn("mods_set_enabled", move |_args: &[&Variant]| {
            let slot = slot.clone();
            let name = name.clone();
            defer_action("mods_set_enabled_now", move || {
                mods_set_enabled(&slot, &name, !enabled);
            });
            Variant::nil()
        });
        toggle.connect("pressed", &callable);
    }
    right.add_child(&toggle);
    // code-instantiated: the per-row delete button.
    let mut remove = factory::glyph_button("trash", &bt("@delete"), "clearNonei");
    remove.set_custom_minimum_size(Vector2::new(50.0, 50.0));
    remove.set_focus_mode(FocusMode::NONE);
    {
        let slot = state.clone();
        let name = mod_name.clone();
        let callable = Callable::from_fn("mods_confirm_delete", move |_args: &[&Variant]| {
            let slot = slot.clone();
            let name = name.clone();
            defer_action("mods_confirm_delete_now", move || {
                mods_confirm_delete(&slot, &name);
            });
            Variant::nil()
        });
        remove.connect("pressed", &callable);
    }
    right.add_child(&remove);

    {
        let mut bound = list.bind_mut();
        bound
            .add(&card)
            .grow_x()
            .height(ROW_HEIGHT)
            .pad(4.0)
            .pad_top(8.0);
        bound.row();
    }

    let update_version = st
        .updates
        .iter()
        .find(|(name, _)| *name == mod_name)
        .map(|(_, version)| version.clone());
    if let Some(version) = update_version {
        // `hasUpdate(mod)`: `Styles.grayt` row, `Icon.download` +
        // `@mods.update.available` (`width(w).height(48f).padTop(-4f)`).
        // code-instantiated: update row from the browser-published set.
        let mut update = mods_gray_button();
        let mut update_row =
            mods_button_row(&update, godot::classes::box_container::AlignmentMode::BEGIN);
        update_row.add_child(&factory::glyph_label(
            "download",
            24,
            styles::ARC_LIGHT_GRAY,
        ));
        update_row.add_child(&mods_row_label(
            &kit::tm_format("@mods.update.available", &[version.as_str()]).to_string(),
        ));
        {
            let name = mod_name.clone();
            let callable = Callable::from_fn("mods_update_mod", move |_args: &[&Variant]| {
                let name = name.clone();
                defer_action("mods_update_mod_now", move || mods_update_mod(&name));
                Variant::nil()
            });
            update.connect("pressed", &callable);
        }
        let mut bound = list.bind_mut();
        bound
            .add(&update)
            .grow_x()
            .height(48.0)
            .pad_left(4.0)
            .pad_right(4.0);
        bound.row();
    }

    if st.open_details == mod_name {
        mods_add_details(state, st, &mod_name);
    }
}

/// `Styles.grayt` (`Styles.java:157-165`): `up = grayPanel`, `over`/`down` =
/// `flatOver`.
pub(super) fn mods_gray_button() -> Gd<Button> {
    // code-instantiated: cards/rows are data-driven.
    let mut button = Button::new_alloc();
    button.set_focus_mode(FocusMode::NONE);
    let up = flat_style(styles::GRAY_PANEL);
    let over = flat_style(styles::FLAT_OVER);
    button.add_theme_stylebox_override("normal", &up);
    button.add_theme_stylebox_override("hover", &over);
    button.add_theme_stylebox_override("pressed", &over);
    button.add_theme_stylebox_override("hover_pressed", &over);
    button.add_theme_stylebox_override("disabled", &up);
    button.add_theme_stylebox_override("focus", &StyleBoxEmpty::new_gd());
    button
}

/// A mouse-transparent content row inside a button (`b.margin(6f)`).
pub(super) fn mods_button_row(
    button: &Gd<Button>,
    alignment: godot::classes::box_container::AlignmentMode,
) -> Gd<HBoxContainer> {
    // code-instantiated: button content rows are data-driven.
    let mut row = HBoxContainer::new_alloc();
    row.set_alignment(alignment);
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, 6.0);
    row.set_offset(Side::RIGHT, -6.0);
    row.add_theme_constant_override("separation", 8);
    row.set_mouse_filter(MouseFilter::IGNORE);
    let mut button = button.clone();
    button.add_child(&row);
    row
}

/// A non-wrapping row label (`_row_label`).
pub(super) fn mods_row_label(markup: &str) -> Gd<MindLabel> {
    let mut label = factory::label(markup);
    label.set_autowrap_mode(AutowrapMode::OFF);
    label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    label.set_mouse_filter(MouseFilter::IGNORE);
    label
}
