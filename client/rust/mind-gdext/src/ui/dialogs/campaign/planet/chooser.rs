// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The first-run planet chooser page.
//! Upstream: core/src/mindustry/ui/dialogs/PlanetDialog.java

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::{Side, VarDictionary};
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::{
    Button, ButtonGroup, Control, HBoxContainer, Label, Panel, StyleBoxEmpty, StyleBoxFlat,
    Texture2D, TextureRect, VBoxContainer,
};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, styles};

use super::reads::*;
use super::util::*;
use super::view_binding::*;
use super::*;

// ---- chooser --------------------------------------------------------------

/// `_apply_full_dialog_margins`: `Styles.fullDialog` insets around the page.
pub(super) fn planet_apply_full_dialog_margins(dialog: &mut MindDialog) {
    let Some(mut panel) = dialog.panel() else {
        return;
    };
    let mut box_ = StyleBoxEmpty::new_gd();
    box_.set_content_margin(Side::LEFT, 3.0);
    box_.set_content_margin(Side::RIGHT, 5.0);
    box_.set_content_margin(Side::TOP, 1.0);
    box_.set_content_margin(Side::BOTTOM, 6.0);
    panel.add_theme_stylebox_override("panel", &box_);
}

/// `_add_space_backdrop`: the full-screen `sprites/space.png` starfield.
pub(super) fn planet_add_space_backdrop(dialog: &mut MindDialog) {
    if dialog.base().get_node_or_null("SpaceBackdrop").is_some() {
        return;
    }
    let Some(mut assets) = kit::assets() else {
        return;
    };
    let Some(texture) = assets.bind_mut().find_loose(GString::from("space")) else {
        return;
    };
    // code-instantiated: single backdrop image; no static scene owns it yet.
    let mut backdrop = TextureRect::new_alloc();
    backdrop.set_name("SpaceBackdrop");
    let texture: Gd<Texture2D> = texture.upcast();
    backdrop.set_texture(&texture);
    backdrop.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    backdrop.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_COVERED);
    backdrop.set_texture_filter(godot::classes::canvas_item::TextureFilter::LINEAR);
    backdrop.set_mouse_filter(MouseFilter::IGNORE);
    let control: Gd<Control> = backdrop.clone().upcast();
    dialog.base_mut().add_child(&control);
    dialog.base_mut().move_child(&control, 0);
}

/// Clears the dialog body (`_clear_root`).
pub(super) fn planet_clear_root(st: &mut PlanetState) {
    if let Some(mut root) = st.root.clone() {
        root.bind_mut().clear();
    }
}

/// `_set_body_centered`: centre or top-align the body and rail.
pub(super) fn planet_set_body_centered(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    centered: bool,
) {
    if let Some(mut body) = dialog.body() {
        body.set_vertical_scroll_mode(if centered {
            godot::classes::scroll_container::ScrollMode::DISABLED
        } else {
            godot::classes::scroll_container::ScrollMode::AUTO
        });
    }
    if let Some(mut content) = dialog.content() {
        content.set_alignment(if centered {
            godot::classes::box_container::AlignmentMode::CENTER
        } else {
            godot::classes::box_container::AlignmentMode::BEGIN
        });
        content.set_v_size_flags(if centered {
            SizeFlags::EXPAND_FILL
        } else {
            SizeFlags::FILL
        });
    }
    if let Some(mut root) = st.root.clone() {
        let mut root = root.bind_mut();
        root.set_table_top(!centered);
        root.set_table_left(!centered);
    }
}

/// The two campaign starting choices in reference order (Serpulo, Erekir).
pub(super) fn planet_campaign_planets(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
) -> Vec<VarDictionary> {
    let views = planet_refresh_views(dialog, st, "");
    let planets = row_rows(&views, "planets");
    let mut out = Vec::new();
    for name in ["serpulo", "erekir"] {
        if let Some(row) = planets.iter().find(|row| row_str(row, "name") == name) {
            out.push(row.clone());
        }
    }
    out
}

/// `_show_select`: the first-run "Select Starting Campaign" chooser.
pub(super) fn planet_show_select(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
) {
    planet_remove_sector_list(st);
    dialog.set_title_key("@campaign.select");
    planet_clear_root(st);
    planet_set_planet_view_active(dialog, st, false);
    planet_set_body_centered(dialog, st, true);
    st.chooser_active = true;
    planet_refresh_views(dialog, st, "");

    let Some(mut root) = st.root.clone() else {
        return;
    };
    let choices = planet_campaign_planets(dialog, st);

    // code-instantiated: the chooser row is data-driven (planet cards).
    let mut row = HBoxContainer::new_alloc();
    row.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    row.add_theme_constant_override("separation", 6);
    root.bind_mut().add(&row).grow_x().pad_top(4.0);
    root.bind_mut().row();

    if let Some(mut title) = dialog.title_node() {
        title.add_theme_font_size_override("font_size", 21);
    }

    // code-instantiated: the caption is a live label updated by card toggles.
    let caption = factory::label(&kit::tm("@campaign.none").to_string());
    {
        let mut label = rich(&caption);
        label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        label.add_theme_font_size_override("normal_font_size", 20);
        // `PlanetDialog.java:181-214`: the caption is a font-20 `Label` whose
        // line advance is the font's own line height — 20 px in the reference
        // frames (PAR-0010 glyphs 448..461 / 468..485; the selected blurb in
        // PAR-0011 keeps the same 20 px advance). `RichTextLabel` adds
        // `line_separation` on top of that height, so leave it at the theme
        // default: the former -8 override collapsed the 20 px line box to a
        // 12 px pitch and drew the two lines on top of each other.
        label.add_theme_constant_override("line_separation", 0);
    }
    {
        let mut control: Gd<Control> = caption.clone().upcast();
        control.set_custom_minimum_size(Vector2::new(440.0, 0.0));
    }

    let mut group = ButtonGroup::new_gd();
    group.set_allow_unpress(true);
    for planet in &choices {
        let planet_name = row_str(planet, "name");
        let mut card = planet_make_card(planet);
        card.set_button_group(&group);
        set_toggle(&card, planet_name == st.selected_planet);
        let state_for_toggle = state.clone();
        let caption_for_toggle = caption.clone();
        let name_for_toggle = planet_name.clone();
        let callable = Callable::from_fn("planet_card_toggled", move |args: &[&Variant]| {
            let pressed = args
                .first()
                .and_then(|value| value.try_to::<bool>().ok())
                .unwrap_or(false);
            if !pressed {
                return;
            }
            let mut st = state_for_toggle.borrow_mut();
            st.selected_planet = name_for_toggle.clone();
            let mut label = rich(&caption_for_toggle);
            label.set_text(&kit::tm(&format!("@campaign.{}", st.selected_planet)));
            if let Some(mut ok) = st.ok_button.clone() {
                ok.set_disabled(false);
            }
        });
        card.connect("toggled", &callable);
        row.add_child(&card);
    }

    // `PlanetDialog.java:181-214`: the caption sits 8 px below the card row
    // (Java glyph top 448 under cards ending at 440). The table cell already
    // contributes 3 px, so a 4 px pad pins the vertically-centred block at the
    // reference card row: y=120 with the two-line chooser caption. (The
    // selected-state blurb is PAR-0011's surface.)
    root.bind_mut()
        .add(&caption)
        .grow_x()
        .max_width(440.0)
        .pad_top(4.0);
    root.bind_mut().row();
    // `PlanetDialog.java:205`: `.width(440f).wrap()`. `MindLabel::ready()` runs on
    // tree entry and `configure_rich_label` resets `autowrap_mode` to OFF, so the
    // wrap must be applied *after* `add()`: with wrap off the fit-content minimum
    // width is the unwrapped paragraph (788 px), `Control.size` clamps up to it and
    // the cell's `max_width(440)` is defeated (PAR-0011: cards y=110 instead of 90).
    // With WORD_SMART the label's minimum width is ~1 px, so the 440 px cell wins
    // and both locale paragraphs wrap to two lines (5 line boxes, 100 px tall).
    {
        let mut label = rich(&caption);
        label.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
    }

    dialog.clear_buttons();
    planet_build_dimmed_footer(dialog, st);
    let target = dialog.to_gd();
    let state_for_ok = state.clone();
    let callable = Callable::from_fn("planet_select_ok", move |_args: &[&Variant]| {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        planet_on_select_ok(&mut bound, &state_for_ok);
    });
    let ok_text = kit::t("@ok").to_string();
    let mut ok = dialog.add_button(&ok_text, Some(callable), "ok", 0.0);
    ok.set_custom_minimum_size(Vector2::new(300.0, 64.0));
    planet_apply_ok_style(&mut ok);
    planet_reflow_ok_row(&mut ok);
    ok.set_disabled(st.selected_planet.is_empty());
    st.ok_button = Some(ok);
}

/// The chooser's `@ok` states (`Styles.button` black fill, 4 px outline).
pub(super) fn planet_apply_ok_style(button: &mut Gd<Button>) {
    for state in ["normal", "hover", "pressed"] {
        let mut box_ = StyleBoxFlat::new_gd();
        box_.set_bg_color(Color::BLACK);
        for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
            box_.set_border_width(side, 4);
        }
        box_.set_border_color(CARD_OUTLINE);
        for corner in [
            godot::builtin::Corner::TOP_LEFT,
            godot::builtin::Corner::TOP_RIGHT,
            godot::builtin::Corner::BOTTOM_LEFT,
            godot::builtin::Corner::BOTTOM_RIGHT,
        ] {
            box_.set_corner_radius(corner, 6);
        }
        box_.set_content_margin(Side::LEFT, 12.0);
        box_.set_content_margin(Side::RIGHT, 12.0);
        button.add_theme_stylebox_override(state, &box_);
    }
    let mut disabled = StyleBoxFlat::new_gd();
    disabled.set_bg_color(Color::from_rgba(0.145, 0.145, 0.145, 1.0));
    for corner in [
        godot::builtin::Corner::TOP_LEFT,
        godot::builtin::Corner::TOP_RIGHT,
        godot::builtin::Corner::BOTTOM_LEFT,
        godot::builtin::Corner::BOTTOM_RIGHT,
    ] {
        disabled.set_corner_radius(corner, 6);
    }
    disabled.set_content_margin(Side::LEFT, 12.0);
    disabled.set_content_margin(Side::RIGHT, 12.0);
    button.add_theme_stylebox_override("disabled", &disabled);
}

/// Java `Button` draws the icon at the left edge and the label centred; the
/// shared icon row centres both, so add the two balancing spacers.
pub(super) fn planet_reflow_ok_row(button: &mut Gd<Button>) {
    if button.get_child_count() == 0 {
        return;
    }
    let Some(first) = button.get_child(0) else {
        return;
    };
    let Ok(mut row) = first.try_cast::<HBoxContainer>() else {
        return;
    };
    if row.get_child_count() != 2 {
        return;
    }
    if let Some(icon) = row.get_child(0)
        && let Ok(mut icon) = icon.try_cast::<Label>()
    {
        icon.add_theme_font_size_override("font_size", 36);
    }
    row.set_alignment(godot::classes::box_container::AlignmentMode::BEGIN);
    let left = spacer(true, 0.0, 0.0);
    let left_control: Gd<Control> = left.clone().upcast();
    row.add_child(&left_control);
    row.move_child(&left_control, 1);
    let right = spacer(true, 0.0, 0.0);
    let right_control: Gd<Control> = right.clone().upcast();
    row.add_child(&right_control);
}

/// `_build_dimmed_footer`: inert 10 %-brightness copies of the page footer.
pub(super) fn planet_build_dimmed_footer(dialog: &mut MindDialog, st: &mut PlanetState) {
    planet_clear_dimmed_footer(st);
    st.footer_entries
        .push(planet_footer_entry(dialog, "@back", "left", true));
    st.footer_entries
        .push(planet_footer_entry(dialog, "@techtree", "tree", false));
}

pub(super) fn planet_clear_dimmed_footer(st: &mut PlanetState) {
    for mut button in st.footer_entries.drain(..) {
        if button.is_instance_valid() {
            button.queue_free();
        }
    }
}

pub(super) fn planet_footer_entry(
    dialog: &mut MindDialog,
    text_key: &str,
    icon: &str,
    left: bool,
) -> Gd<Button> {
    // code-instantiated: dimmed copies of the planet page's footer entries.
    let text = kit::t(text_key).to_string();
    let mut button = factory::icon_button(
        icon,
        &text,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    button.set_custom_minimum_size(FOOTER_BUTTON);
    button.set_anchors_preset(if left {
        LayoutPreset::BOTTOM_LEFT
    } else {
        LayoutPreset::BOTTOM_RIGHT
    });
    button.set_offset(Side::LEFT, if left { 5.0 } else { -205.0 });
    button.set_offset(Side::RIGHT, if left { 205.0 } else { -5.0 });
    button.set_offset(Side::TOP, -59.0);
    button.set_offset(Side::BOTTOM, -5.0);
    button.set_modulate(Color::from_rgba(0.1, 0.1, 0.1, 1.0));
    button.set_mouse_filter(MouseFilter::IGNORE);
    button.set_focus_mode(godot::classes::control::FocusMode::NONE);
    let control: Gd<Control> = button.clone().upcast();
    dialog.base_mut().add_child(&control);
    // Above the `Dim` overlay, below the centred panel content.
    dialog.base_mut().move_child(&control, 2);
    button
}

/// One `PlanetDialog` chooser card (`Styles.togglet` + planet art).
pub(super) fn planet_make_card(planet: &VarDictionary) -> Gd<Button> {
    // code-instantiated: planet card layout is data-driven (no static scene).
    let mut card = factory::button("");
    card.set_toggle_mode(true);
    card.set_custom_minimum_size(CARD_SIZE);
    planet_apply_card_style(&mut card);
    let texture = planet_texture(planet);
    if let Some(texture) = texture.clone() {
        // code-instantiated: vendored `sprites/planets/<name>.png`.
        let mut image = TextureRect::new_alloc();
        image.set_texture(&texture);
        image.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        image.set_offset(Side::LEFT, 5.0);
        image.set_offset(Side::TOP, 31.0);
        image.set_offset(Side::RIGHT, -5.0);
        image.set_offset(Side::BOTTOM, -12.0);
        image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
        image.set_texture_filter(godot::classes::canvas_item::TextureFilter::LINEAR);
        image.set_mouse_filter(MouseFilter::IGNORE);
        let control: Gd<Control> = image.clone().upcast();
        card.add_child(&control);
    }
    // code-instantiated: card name column (label + optional disc).
    let mut column = VBoxContainer::new_alloc();
    column.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    column.set_offset(Side::LEFT, 5.0);
    column.set_offset(Side::TOP, 8.0);
    column.set_offset(Side::RIGHT, -5.0);
    column.set_offset(Side::BOTTOM, -8.0);
    column.set_alignment(godot::classes::box_container::AlignmentMode::BEGIN);
    column.set_mouse_filter(MouseFilter::IGNORE);
    card.add_child(&column);
    let name_label = factory::label(&planet_localized_planet(planet));
    {
        let mut label = rich(&name_label);
        label.add_theme_color_override("default_color", styles::ACCENT);
        label.add_theme_font_size_override("normal_font_size", 21);
        label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        label.set_mouse_filter(MouseFilter::IGNORE);
    }
    column.add_child(&name_label);
    if texture.is_none() {
        // code-instantiated: tinted disc fallback when the art is missing.
        let mut disc = Panel::new_alloc();
        disc.set_custom_minimum_size(Vector2::new(DISC_SIZE, DISC_SIZE));
        disc.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        disc.add_theme_stylebox_override("panel", &planet_disc_style(planet));
        disc.set_mouse_filter(MouseFilter::IGNORE);
        column.add_child(&disc);
    }
    card
}

pub(super) fn planet_apply_card_style(card: &mut Gd<Button>) {
    card.add_theme_stylebox_override("normal", &planet_card_box(CARD_OUTLINE));
    card.add_theme_stylebox_override("hover", &planet_card_box(styles::ACCENT));
    card.add_theme_stylebox_override("pressed", &planet_card_box(Color::WHITE));
    card.add_theme_stylebox_override("hover_pressed", &planet_card_box(Color::WHITE));
    card.add_theme_stylebox_override("disabled", &planet_card_box(CARD_OUTLINE));
}

pub(super) fn planet_card_box(border: Color) -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(Color::BLACK);
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        box_.set_border_width(side, 4);
    }
    box_.set_border_color(border);
    for corner in [
        godot::builtin::Corner::TOP_LEFT,
        godot::builtin::Corner::TOP_RIGHT,
        godot::builtin::Corner::BOTTOM_LEFT,
        godot::builtin::Corner::BOTTOM_RIGHT,
    ] {
        box_.set_corner_radius(corner, 4);
    }
    box_.set_content_margin(Side::LEFT, 5.0);
    box_.set_content_margin(Side::RIGHT, 5.0);
    box_.set_content_margin(Side::TOP, 6.0);
    box_.set_content_margin(Side::BOTTOM, 5.0);
    box_
}

/// Vendored `sprites/planets/<name>.png` (`PlanetDialog.planetTextures`).
pub(super) fn planet_texture(planet: &VarDictionary) -> Option<Gd<Texture2D>> {
    let mut assets = kit::assets()?;
    let planet_name = row_str(planet, "name");
    let texture = assets
        .bind_mut()
        .find_loose(GString::from(format!("planets/{planet_name}").as_str()))?;
    Some(texture.upcast())
}

/// `_disc_style`: the planet's [`color`] disc with an accent ring.
pub(super) fn planet_disc_style(planet: &VarDictionary) -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    let color = row_str(planet, "color");
    box_.set_bg_color(Color::from_string(&color).unwrap_or(Color::WHITE));
    let radius = (DISC_SIZE * 0.5) as i32;
    for corner in [
        godot::builtin::Corner::TOP_LEFT,
        godot::builtin::Corner::TOP_RIGHT,
        godot::builtin::Corner::BOTTOM_LEFT,
        godot::builtin::Corner::BOTTOM_RIGHT,
    ] {
        box_.set_corner_radius(corner, radius);
    }
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        box_.set_border_width(side, 2);
    }
    box_.set_border_color(styles::ACCENT);
    box_
}

/// `_on_select_ok`: `state.planet = selected; selectSector(startSector)`.
pub(super) fn planet_on_select_ok(dialog: &mut MindDialog, state: &Rc<RefCell<PlanetState>>) {
    let mut st = state.borrow_mut();
    if st.selected_planet.is_empty() {
        return;
    }
    setting_put_flag("campaignselect", true);
    let selected_planet = st.selected_planet.clone();
    let planet = planet_find_planet_row(dialog, &mut st, &selected_planet);
    if !planet.is_empty() {
        st.selected_sector = row_i64(&planet, "start_sector");
    }
    let selected = st.selected_planet.clone();
    planet_show_planet(dialog, state, &mut st, &selected, true);
    // `PlanetDialog.java:209-210`: after the starting campaign is chosen, Java
    // centres the camera on the planet's start sector (`lookAt(startSector)`)
    // before selecting it. The port dropped that call, so the fresh chooser
    // path kept the default polar camera and Ground Zero sat near the limb
    // instead of the reference framing (PAR-0018). `planet_show_planet` has
    // already stored the start tile in `selected_tile`.
    if let Some(mut view) = planet_view() {
        let tile = view.get("selected_tile").try_to::<i64>().unwrap_or(-1);
        if tile >= 0 {
            planet_view_call(&mut view, "look_at_tile", &[tile.to_variant()]);
        }
    }
}

/// `PlanetDialog.java:216-222`: the one-time Serpulo remap notice.
pub(super) fn planet_maybe_show_remap_notice(dialog: &mut MindDialog) {
    let Some(mut campaign) = kit::campaign() else {
        return;
    };
    if !campaign.has_method("take_serpulo_remaps_notice") {
        return;
    }
    if !campaign
        .call("take_serpulo_remaps_notice", &[])
        .try_to::<bool>()
        .unwrap_or(false)
    {
        return;
    }
    let mut context = VarDictionary::new();
    context.set("title", &kit::t("@campaign.rework.title"));
    context.set("text", &kit::tm("@campaign.rework.serpulo"));
    context.set("ok", true);
    context.set("keep_previous", true);
    dialog.open_dialog("full_text", &json_stringify(&context));
}
