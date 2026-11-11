// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The planet page: planetary rail, sector list and planet switching.
//! Upstream: core/src/mindustry/ui/dialogs/PlanetDialog.java

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::{Side, VarDictionary};
use godot::classes::control::{GrowDirection, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::{
    Button, Control, HBoxContainer, LineEdit, PanelContainer, ScrollContainer, StyleBoxFlat,
    TextureRect, VBoxContainer,
};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;
use mind_core::ui::campaign::{PlanetView, SectorView};

use crate::ui::dialog::MindDialog;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindTable, styles};

use super::chooser::*;
use super::reads::*;
use super::sector_panel::*;
use super::stage::*;
use super::util::*;
use super::view_binding::*;
use super::*;

// ---- planet page ----------------------------------------------------------

/// `PlanetDialog.show`: adopt the running sector's planet.
pub(super) fn planet_adopt_current_sector(dialog: &mut MindDialog, st: &mut PlanetState) {
    let Some(campaign) = dialog.campaign() else {
        return;
    };
    let state = campaign.bind().sector_state();
    if !state.campaign {
        return;
    }
    let planet = state.planet;
    if planet.is_empty() {
        return;
    }
    st.selected_planet = planet.clone();
    planet_refresh_views(dialog, st, &planet);
}

/// Names of the landable planets with at least one based sector.
pub(super) fn planet_planets_with_base(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
) -> Vec<String> {
    let Some(views) = planet_refresh_views(dialog, st, "") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for planet in &views.planets {
        if planet.name.is_empty() || !planet.has_grid {
            continue;
        }
        let Some(live) = planet_live_views(dialog, &planet.name) else {
            continue;
        };
        for sector in &live.sectors {
            if sector.has_base {
                out.push(planet.name.clone());
                break;
            }
        }
    }
    out
}

/// `_boot_planet`: `lastplanet`, else the first based planet, else serpulo.
pub(super) fn planet_boot_planet(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    based: &[String],
) -> String {
    let last = setting_text("lastplanet", "serpulo");
    let accessible = planet_find_planet(dialog, st, &last).is_some_and(|planet| planet.accessible);
    if accessible || based.iter().any(|name| name == &last) {
        return last;
    }
    if let Some(first) = based.first() {
        return first.clone();
    }
    String::from("serpulo")
}

/// `refresh_page`: re-runs the planet page for the selected planet.
pub(super) fn planet_show_planet(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    planet_name: &str,
    select_default: bool,
) {
    if st.last_planet_written != planet_name {
        setting_put_text("lastplanet", planet_name);
        st.last_planet_written = planet_name.to_string();
    }
    st.selected_planet = planet_name.to_string();
    st.chooser_active = false;
    st.new_presets.clear();
    planet_clear_dimmed_footer(st);
    dialog.set_title_literal("");
    planet_clear_root(st);
    planet_set_planet_view_active(dialog, st, true);
    planet_set_body_centered(dialog, st, false);
    planet_refresh_views(dialog, st, planet_name);

    if let Some(mut view) = planet_view() {
        view.bind_mut().set_planet(gs(planet_name));
        let sectors = planet_sectors(dialog, st, planet_name);
        view.bind_mut().set_sector_views(&sectors);
    }

    // code-instantiated: the planet rail is the data-driven Planets list.
    let Some(mut root) = st.root.clone() else {
        return;
    };
    let mut rail = MindTable::new_alloc();
    rail.set_custom_minimum_size(Vector2::new(208.0, 0.0));
    root.bind_mut()
        .add(&rail)
        .pad(0.0)
        .align(kit::Align::Center);
    planet_build_rail(dialog, state, st, &mut rail, planet_name);
    planet_build_sector_list(dialog, state, st, planet_name);

    let mut selected = planet_find_sector(dialog, st, planet_name, st.selected_sector);
    if selected.is_none() && select_default {
        selected = planet_default_sector(dialog, st, planet_name);
        st.selected_sector = selected
            .as_ref()
            .map(|sector| i64::from(sector.id))
            .unwrap_or(-1);
    }
    if let Some(mut view) = planet_view() {
        match selected.as_ref() {
            None => view.bind_mut().clear_selection(),
            Some(selected) => {
                let tile = planet_view_tile(&view, selected);
                view.bind_mut().focus_tile(tile as i32);
                view.bind_mut()
                    .set_sector_icon(gs(selected.preset.as_deref().unwrap_or("")));
            }
        }
    }

    planet_build_action_row(dialog, state, st, selected.as_ref());
    let mut rail_sort = rail.clone();
    st.deferred.push(move || {
        rail_sort.bind_mut().sort_now();
    });
    let target = dialog.to_gd();
    let state_for_complete = state.clone();
    st.deferred.push(move || {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        planet_maybe_show_campaign_complete(&mut bound, &state_for_complete);
    });
}

/// `_default_sector`: start sector, else Ground Zero, else the first unlocked.
pub(super) fn planet_default_sector(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    planet_name: &str,
) -> Option<SectorView> {
    let sectors = planet_sectors(dialog, st, planet_name);
    if sectors.is_empty() {
        return None;
    }
    let mut start = -1i64;
    if let Some(view) = planet_view() {
        let data = planet_view_data(&view);
        if !data.is_empty() {
            start = row_i64_or(&data, "start_sector", -1);
        }
        for sector in &sectors {
            if planet_view_tile(&view, sector) == start {
                return Some(sector.clone());
            }
        }
    }
    if let Some(sector) = sectors
        .iter()
        .find(|sector| sector.preset.as_deref() == Some("groundZero"))
    {
        return Some(sector.clone());
    }
    if let Some(sector) = sectors.iter().find(|sector| !sector.locked) {
        return Some(sector.clone());
    }
    sectors.first().cloned()
}

/// `_maybe_show_campaign_complete`: the completion screen after the last capture.
pub(super) fn planet_maybe_show_campaign_complete(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
) {
    let mut st = state.borrow_mut();
    let planet_name = st.selected_planet.clone();
    let Some(views) = planet_ensure_views(dialog, &mut st, &planet_name) else {
        return;
    };
    let last_captured = views
        .sectors
        .iter()
        .any(|sector| sector.is_last && sector.captured);
    if !last_captured {
        return;
    }
    let mut planet = views.complete.planet.clone();
    if planet.is_empty() {
        planet = views.planet.clone();
    }
    if planet.is_empty() || planet == st.complete_announced {
        return;
    }
    if let Some(complete_dialog) = dialog_node("campaign_complete")
        && complete_dialog.has_method("is_shown")
        && complete_dialog
            .clone()
            .call("is_shown", &[])
            .try_to::<bool>()
            .unwrap_or(false)
    {
        return;
    }
    st.complete_announced = planet;
    // The completion page reads the context keys `localized`/`planet`/`color`/
    // `playtime_ms`; `CampaignCompleteView` serializes to exactly that shape.
    let context = match serde_json::to_string(&views.complete) {
        Ok(context) => context,
        Err(_) => return,
    };
    open_dialog_json("campaign_complete", &context);
}

/// `_build_sector_list`: the collapsible top-right based-sector list.
pub(super) fn planet_build_sector_list(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    planet_name: &str,
) {
    planet_remove_sector_list(st);
    let sectors = planet_sectors(dialog, st, planet_name);
    let mut based: Vec<SectorView> = Vec::new();
    let mut attacked = 0i64;
    for sector in &sectors {
        if sector.attacked {
            attacked += 1;
        }
        if sector.has_base {
            based.push(sector.clone());
        }
    }
    if based.is_empty() {
        return;
    }
    based.sort_by(|a, b| {
        if a.attacked != b.attacked {
            return if a.attacked {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        b.stats.playtime_ms.cmp(&a.stats.playtime_ms)
    });

    // code-instantiated: rebuilt with the planet view; content is the live
    // based-sector set. Java: `sectorList.background(Styles.black8)`.
    let mut backdrop = PanelContainer::new_alloc();
    backdrop.set_name("SectorListBackdrop");
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(styles::BLACK_8);
    backdrop.add_theme_stylebox_override("panel", &box_);
    backdrop.set_anchors_preset(LayoutPreset::TOP_RIGHT);
    backdrop.set_h_grow_direction(GrowDirection::BEGIN);
    backdrop.set_offset(Side::LEFT, -295.0);
    backdrop.set_offset(Side::RIGHT, -5.0);
    backdrop.set_offset(Side::TOP, 3.0);
    backdrop.set_custom_minimum_size(Vector2::new(290.0, 0.0));
    let control: Gd<Control> = backdrop.clone().upcast();
    dialog.base_mut().add_child(&control);
    st.sector_list_backdrop = Some(backdrop.clone());

    // code-instantiated: the toggle + entry list of the sector list.
    let mut list = VBoxContainer::new_alloc();
    list.set_name("SectorList");
    list.add_theme_constant_override("separation", 0);
    backdrop.add_child(&list);
    st.sector_list = Some(list.clone());

    let mut toggle = Button::new_alloc();
    toggle.set_toggle_mode(true);
    set_toggle(&toggle, st.sectors_shown);
    toggle.set_custom_minimum_size(Vector2::new(290.0, 60.0));
    dialog.apply_pane_button(&mut toggle, OUTLINE_GRAY, 4.0);
    let mut raw = kit::t("@sectorlist").to_string();
    if attacked > 0 {
        let count = format!("[red]{attacked}[]");
        let line = match kit::assets() {
            Some(assets) => {
                let mut args = PackedStringArray::new();
                args.push(&count);
                assets
                    .bind()
                    .bundle_format(GString::from("sectorlist.attacked"), args)
                    .to_string()
            }
            None => count.clone(),
        };
        raw.push_str(&format!("\n[red]\u{26a0}[lightgray] {line}"));
    }
    // code-instantiated: toggle content row (arrow glyph + caption).
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, 7.0);
    row.set_offset(Side::RIGHT, -7.0);
    row.add_theme_constant_override("separation", 8);
    row.set_mouse_filter(MouseFilter::IGNORE);
    toggle.add_child(&row);
    let arrow = glyph_control(
        if st.sectors_shown {
            "up-open"
        } else {
            "down-open"
        },
        22.0,
    );
    row.add_child(&arrow);
    let caption = factory::label(&kit::markup_text(&raw).to_string());
    {
        let mut label = rich(&caption);
        label.set_fit_content(true);
        label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        label.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
        label.set_autowrap_mode(godot::classes::text_server::AutowrapMode::OFF);
        label.set_mouse_filter(MouseFilter::IGNORE);
    }
    {
        let mut control: Gd<Control> = caption.clone().upcast();
        control.set_h_size_flags(SizeFlags::EXPAND_FILL);
    }
    row.add_child(&caption);
    let target = dialog.to_gd();
    let state_for_toggle = state.clone();
    let callable = Callable::from_fn("planet_sector_list_toggle", move |_args: &[&Variant]| {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        let mut st = state_for_toggle.borrow_mut();
        st.sectors_shown = !st.sectors_shown;
        let planet = st.selected_planet.clone();
        planet_build_sector_list(&mut bound, &state_for_toggle, &mut st, &planet);
    });
    toggle.connect("pressed", &callable);
    list.add_child(&toggle);
    if !st.sectors_shown {
        return;
    }

    let mut panel = panel_with_box(pane_box(
        Color::from_rgba(0.0, 0.0, 0.0, 0.8),
        Color::from_rgba(0.0, 0.0, 0.0, 0.0),
        0,
        0,
    ));
    list.add_child(&panel);
    // code-instantiated: the search + scrolling entry column.
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 3);
    panel.add_child(&column);
    let mut search_row = HBoxContainer::new_alloc();
    let zoom = glyph_control("zoom", 22.0);
    search_row.add_child(&zoom);
    let mut field = LineEdit::new_alloc();
    field.set_text(&GString::from(st.sector_search.as_str()));
    field.set_custom_minimum_size(Vector2::new(0.0, 50.0));
    field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    search_row.add_child(&field);
    column.add_child(&search_row);
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(godot::classes::scroll_container::ScrollMode::DISABLED);
    scroll.set_custom_minimum_size(Vector2::new(0.0, 320.0));
    column.add_child(&scroll);
    // code-instantiated: one underlined row per based sector.
    let mut entries = VBoxContainer::new_alloc();
    entries.set_h_size_flags(SizeFlags::EXPAND_FILL);
    scroll.add_child(&entries);
    let target = dialog.to_gd();
    let state_for_search = state.clone();
    let entries_for_search = entries.clone();
    let based_for_search = based.clone();
    let callable = Callable::from_fn("planet_sector_search", move |args: &[&Variant]| {
        let text = args
            .first()
            .and_then(|value| value.try_to::<GString>().ok())
            .map(|value| value.to_string())
            .unwrap_or_default();
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        let mut st = state_for_search.borrow_mut();
        st.sector_search = text;
        planet_fill_sector_entries(
            &mut bound,
            &state_for_search,
            &mut st,
            &entries_for_search,
            &based_for_search,
        );
    });
    field.connect("text_changed", &callable);
    planet_fill_sector_entries(dialog, state, st, &entries, &based);
}

/// `rebuildList.readd`: one `Styles.underlineb` row per matching based sector.
pub(super) fn planet_fill_sector_entries(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    entries: &Gd<VBoxContainer>,
    based: &[SectorView],
) {
    let mut entries = entries.clone();
    for child in entries.get_children().iter_shared() {
        let mut child = child.clone();
        child.queue_free();
    }
    let needle = st.sector_search.to_lowercase();
    for sector in based {
        let title = planet_localized_sector(sector);
        if !needle.is_empty() && !title.to_lowercase().contains(&needle) {
            continue;
        }
        let mut entry = Button::new_alloc();
        entry.set_toggle_mode(true);
        set_toggle(&entry, i64::from(sector.id) == st.selected_sector);
        entry.set_custom_minimum_size(Vector2::new(0.0, 40.0));
        dialog.apply_pane_button(&mut entry, OUTLINE_GRAY, 3.0);
        let preset = sector.preset.as_deref().unwrap_or("");
        let icon: Option<Gd<Control>> = if sector.attacked {
            Some(factory::glyph_label("warn", 18, styles::ACCENT).upcast())
        } else if !preset.is_empty() {
            let mut image = factory::image(&format!("sector-{preset}"));
            image.set_custom_minimum_size(Vector2::new(20.0, 20.0));
            image.set_mouse_filter(MouseFilter::IGNORE);
            Some(image.upcast())
        } else {
            None
        };
        dialog.build_button_content(&mut entry, icon, &title);
        let sector_id = i64::from(sector.id);
        let target = dialog.to_gd();
        let state_for_pick = state.clone();
        let callable = Callable::from_fn("planet_sector_entry", move |_args: &[&Variant]| {
            let mut bound = target.clone();
            let mut bound = bound.bind_mut();
            let mut st = state_for_pick.borrow_mut();
            st.selected_sector = sector_id;
            let planet = st.selected_planet.clone();
            planet_show_planet(&mut bound, &state_for_pick, &mut st, &planet, true);
        });
        entry.connect("pressed", &callable);
        entries.add_child(&entry);
    }
    if entries.get_child_count() == 0 {
        let none = factory::label(&kit::tm("@none.found").to_string());
        entries.add_child(&none);
    }
}

pub(super) fn planet_remove_sector_list(st: &mut PlanetState) {
    if let Some(mut backdrop) = st.sector_list_backdrop.take()
        && backdrop.is_instance_valid()
    {
        backdrop.queue_free();
    }
    st.sector_list = None;
}

/// The left planet/difficulty rail (`PlanetDialog.java:732-739`).
pub(super) fn planet_build_rail(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    rail: &mut Gd<MindTable>,
    planet_name: &str,
) {
    let views = planet_refresh_views(dialog, st, planet_name);
    let planets: Vec<PlanetView> = views.map(|views| views.planets).unwrap_or_default();
    let mut first_planet = true;
    for rail_name in ["erekir", "serpulo"] {
        let Some(planet) = planets.iter().find(|row| row.name == rail_name) else {
            continue;
        };
        if !planet.visible {
            continue;
        }
        let button = planet_make_planet_button(dialog, state, planet, rail_name == planet_name);
        rail.bind_mut()
            .add(&button)
            .grow_x()
            .pad_left(0.0)
            .pad_right(0.0)
            .pad_bottom(0.0)
            .pad_top(if first_planet { 2.0 } else { 0.0 });
        first_planet = false;
        rail.bind_mut().row();
    }
    // code-instantiated: `@campaign.difficulty` opens the campaign rules dialog.
    let mut difficulty = Button::new_alloc();
    difficulty.set_theme_type_variation("flatTogglet");
    difficulty.set_custom_minimum_size(Vector2::new(208.0, 40.0));
    dialog.apply_pane_button(&mut difficulty, OUTLINE_GRAY, 4.0);
    let glyph = glyph_control("book", 20.0);
    let glyph_control_gd: Gd<Control> = glyph.upcast();
    let label = kit::t("@campaign.difficulty").to_string();
    dialog.build_button_content(&mut difficulty, Some(glyph_control_gd), &label);
    let target = dialog.to_gd();
    let planet_for_rules = planet_name.to_string();
    let callable = Callable::from_fn("planet_campaign_rules", move |_args: &[&Variant]| {
        let mut bound = target.clone();
        let bound = bound.bind_mut();
        let mut context = VarDictionary::new();
        context.set("planet", &GString::from(planet_for_rules.as_str()));
        bound.open_dialog("campaign_rules", &json_stringify(&context));
    });
    difficulty.connect("pressed", &callable);
    let selected_planet = planets.iter().find(|row| row.name == planet_name);
    difficulty.set_visible(
        selected_planet.is_none_or(|planet| planet.allow_campaign_rules)
            && st.launch_mode != "planetLaunch",
    );
    rail.bind_mut()
        .add(&difficulty)
        .grow_x()
        .pad_left(0.0)
        .pad_right(0.0)
        .pad_top(13.0)
        .pad_bottom(2.0);
    rail.bind_mut().row();
}

/// One rail row: planet art + localized name, accent-tinted checked border.
pub(super) fn planet_make_planet_button(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    planet: &PlanetView,
    selected: bool,
) -> Gd<Button> {
    // code-instantiated: rail rows carry the vendored planet art plus the
    // localized name, so each entry is built at runtime.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("flatTogglet");
    button.set_toggle_mode(true);
    set_toggle(&button, selected);
    button.set_custom_minimum_size(Vector2::new(208.0, 44.0));
    dialog.apply_pane_button(&mut button, OUTLINE_GRAY, 0.0);
    let planet_color = Color::from_string(&planet.color).unwrap_or(Color::WHITE);
    let checked = Color::from_rgba(
        styles::ACCENT.r * planet_color.r,
        styles::ACCENT.g * planet_color.g,
        styles::ACCENT.b * planet_color.b,
        1.0,
    );
    let checked_box = pane_box(Color::BLACK, checked, 4, 2);
    button.add_theme_stylebox_override("pressed", &checked_box);
    button.add_theme_stylebox_override("hover_pressed", &checked_box);
    let icon: Option<Gd<Control>> = planet_texture(planet).map(|texture| {
        let mut image = TextureRect::new_alloc();
        image.set_texture(&texture);
        image.set_custom_minimum_size(Vector2::new(20.0, 20.0));
        image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
        image.set_mouse_filter(MouseFilter::IGNORE);
        image.upcast()
    });
    let label = planet_localized_planet(planet);
    dialog.build_button_content(&mut button, icon, &label);
    let planet_name = planet.name.clone();
    let target = dialog.to_gd();
    let state_for_switch = state.clone();
    let callable = Callable::from_fn("planet_rail_switch", move |_args: &[&Variant]| {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        planet_switch_planet(&mut bound, &state_for_switch, &planet_name);
    });
    button.connect("pressed", &callable);
    button
}

/// `PlanetDialog.viewPlanet`: switching planets clears the selection.
pub(super) fn planet_switch_planet(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    planet_name: &str,
) {
    let mut st = state.borrow_mut();
    if st.selected_planet == planet_name {
        return;
    }
    st.selected_sector = -1;
    planet_show_planet(dialog, state, &mut st, planet_name, false);
}
