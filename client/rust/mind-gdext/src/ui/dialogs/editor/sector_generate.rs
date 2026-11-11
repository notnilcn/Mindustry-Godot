// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The sector generate dialog controller.
//! Upstream: editor/SectorGenerateDialog.java

use crate::gdutil::variant::{
    dict_i64_or, dict_text_or, parse_array, to_bool_lenient, to_i64_lenient,
};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::editor::util::*;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindLabel, styles};
use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::{
    Button, CenterContainer, ColorRect, Control, GridContainer, HBoxContainer, LineEdit,
    PanelContainer, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "sector_generate" => Some(Box::new(SectorGenerateDialog::default())),
        _ => None,
    }
}

/// The `sector_generate` view state.
struct SectorGenerateState {
    planet: String,
    sector: i64,
    seed: i64,
    planet_button: Option<Gd<Button>>,
    sector_field: Option<Gd<LineEdit>>,
    seed_field: Option<Gd<LineEdit>>,
    size_label: Option<Gd<MindLabel>>,
}

impl Default for SectorGenerateState {
    fn default() -> Self {
        Self {
            planet: String::from("erekir"),
            sector: 0,
            seed: 0,
            planet_button: None,
            sector_field: None,
            seed_field: None,
            size_label: None,
        }
    }
}

/// `@editor.sectorgenerate` — the sector/seed generator form.
#[derive(Default)]
pub struct SectorGenerateDialog {
    state: Rc<RefCell<SectorGenerateState>>,
}

impl DialogController for SectorGenerateDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.sectorgenerate");
        dialog.should_pause = false;
        dialog.clear_buttons();
        // `addCloseButton()` + `buttons.button("@editor.apply", Icon.ok, ...)`.
        dialog.add_default_close_button();
        let dialog_gd = dialog.to_gd();
        let state = self.state.clone();
        let apply = Callable::from_fn("sector_generate_apply", move |_args: &[&Variant]| {
            sector_generate_apply(&dialog_gd, &state);
        });
        let text = kit::t("@editor.apply").to_string();
        dialog.add_button(&text, Some(apply), "ok", 210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        sector_generate_build(dialog, &self.state);
    }
}

/// `setup()` (`:23-85`).
fn sector_generate_build(dialog: &mut MindDialog, state: &Rc<RefCell<SectorGenerateState>>) {
    dialog.clear_content();
    let mut root = dialog.content_table();

    // `cont.add("@editor.planet"); cont.button(planet.localizedName, ...)`.
    let planet_button = sector_generate_planet_picker(dialog, state);
    let planet_control: Gd<Control> = planet_button.clone().upcast();
    let row = sector_generate_row(&kit::t("@editor.planet").to_string(), &planet_control);
    root.bind_mut().add(&row).pad(4.0);
    root.bind_mut().row();

    // `cont.field(sector + "", ...).valid(...)`.
    let mut sector_field = factory::field("");
    sector_field.set_text(&GString::from(state.borrow().sector.to_string().as_str()));
    sector_field.set_custom_minimum_size(Vector2::new(200.0, 0.0));
    let dialog_gd = dialog.to_gd();
    let state_for_sector = state.clone();
    let callable = Callable::from_fn("sector_field_changed", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        sector_generate_on_sector_changed(&dialog_gd, &state_for_sector, &text);
    });
    sector_field.connect("text_changed", &callable);
    let sector_control: Gd<Control> = sector_field.clone().upcast();
    let row = sector_generate_row(&kit::t("@editor.sector").to_string(), &sector_control);
    root.bind_mut().add(&row).pad(4.0);
    root.bind_mut().row();
    state.borrow_mut().sector_field = Some(sector_field);

    // `cont.field(seed + "", ...).valid(Strings::canParseInt)`.
    let mut seed_field = factory::field("");
    seed_field.set_text(&GString::from(state.borrow().seed.to_string().as_str()));
    seed_field.set_custom_minimum_size(Vector2::new(200.0, 0.0));
    let state_for_seed = state.clone();
    let callable = Callable::from_fn("seed_field_changed", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        if !text.is_empty() && text != "-" && is_valid_int(&text) {
            state_for_seed.borrow_mut().seed = text.parse().unwrap_or(0);
        }
    });
    seed_field.connect("text_changed", &callable);
    let seed_control: Gd<Control> = seed_field.clone().upcast();
    let row = sector_generate_row(&kit::t("@editor.seed").to_string(), &seed_control);
    root.bind_mut().add(&row).pad(4.0);
    root.bind_mut().row();
    state.borrow_mut().seed_field = Some(seed_field);

    // `cont.label(() -> "[ " + size + "x" + size + " ]").color(Pal.accent)`.
    let mut size_label = factory::label("");
    size_label.add_theme_color_override("font_color", styles::ACCENT);
    size_label.set_horizontal_alignment(HorizontalAlignment::CENTER);
    root.bind_mut().add(&size_label).pad(5.0);
    state.borrow_mut().size_label = Some(size_label);
    sector_generate_refresh_size(state);
}

/// One dialog row (`planet`/`sector`/`seed`), data-driven.
fn sector_generate_row(label_text: &str, control: &Gd<Control>) -> Gd<HBoxContainer> {
    // code-instantiated: one labelled form row (planet/sector/seed).
    let mut row = factory::hbox(10);
    let label = factory::label(label_text);
    row.add_child(&label);
    row.add_child(control);
    row
}

/// `cont.button(planet.localizedName, ...)`: the planet-picker entry button.
fn sector_generate_planet_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SectorGenerateState>>,
) -> Gd<Button> {
    let mut button = factory::button(&sector_generate_planet_name(state));
    button.set_custom_minimum_size(Vector2::new(200.0, 40.0));
    let dialog_gd = dialog.to_gd();
    let state_for = state.clone();
    let callable = Callable::from_fn("sector_planet_picker", move |_args: &[&Variant]| {
        if !dialog_gd.is_instance_valid() {
            return;
        }
        let mut node = dialog_gd.clone();
        let mut bound = node.bind_mut();
        sector_generate_open_picker(&mut bound, &state_for);
    });
    button.connect("pressed", &callable);
    state.borrow_mut().planet_button = Some(button.clone());
    button
}

/// `_planet_label(_planet)`: `planet.<name>.name` from the bundle.
fn sector_generate_planet_name(state: &Rc<RefCell<SectorGenerateState>>) -> String {
    let planet = state.borrow().planet.clone();
    planet_label(&planet)
}

/// `_planet_label(planet)`: `MindAssets.bundle_get("planet.<name>.name")`.
fn planet_label(planet: &str) -> String {
    let Some(assets) = kit::assets() else {
        return String::from(planet);
    };
    let key = format!("planet.{planet}.name");
    let value = assets.bind().bundle_get(GString::from(key.as_str()));
    let value = value.to_string();
    if value.is_empty() || value == key {
        return String::from(planet);
    }
    value
}

/// `_open_planet_picker`: the `BaseDialog` planet list (:33-57) as a local
/// overlay with 4 toggle columns.
fn sector_generate_open_picker(dialog: &mut MindDialog, state: &Rc<RefCell<SectorGenerateState>>) {
    let Some(value) = editor_call("sector_planets_json", &[]) else {
        return;
    };
    let planets = parse_array(&value.to::<GString>().to_string());
    if planets.is_empty() {
        return;
    }
    let current = state.borrow().planet.clone();
    // code-instantiated: per-action picker panels have no static scene.
    let mut overlay = ColorRect::new_alloc();
    overlay.set_name("PlanetPicker");
    overlay.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.8));
    overlay.set_anchors_preset(LayoutPreset::FULL_RECT);
    overlay.set_mouse_filter(MouseFilter::STOP);
    let overlay_control: Gd<Control> = overlay.clone().upcast();
    dialog.base_mut().add_child(&overlay_control);

    let mut center = CenterContainer::new_alloc();
    center.set_anchors_preset(LayoutPreset::FULL_RECT);
    center.set_mouse_filter(MouseFilter::IGNORE);
    overlay.add_child(&center);
    let mut panel = PanelContainer::new_alloc();
    panel.set_theme_type_variation("defaultDialog");
    center.add_child(&panel);
    let mut box_ = VBoxContainer::new_alloc();
    panel.add_child(&box_);
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(4);
    grid.add_theme_constant_override("h_separation", 4);
    grid.add_theme_constant_override("v_separation", 4);
    box_.add_child(&grid);

    for value in planets.iter_shared() {
        let Ok(row) = value.try_to::<VarDictionary>() else {
            continue;
        };
        let planet_name = dict_text_or(&row, "name");
        let mut button = factory::button(&planet_label(&planet_name));
        button.set_theme_type_variation("flatTogglet");
        button.set_custom_minimum_size(Vector2::new(110.0, 45.0));
        button.set_toggle_mode(true);
        button.set_pressed(planet_name == current);
        let dialog_gd = dialog.to_gd();
        let state_for = state.clone();
        let overlay_for_pick: Gd<Control> = overlay_control.clone();
        let name_for_pick = planet_name.clone();
        let callable = Callable::from_fn("sector_pick_planet", move |_args: &[&Variant]| {
            let mut overlay = overlay_for_pick.clone();
            overlay.queue_free();
            if !dialog_gd.is_instance_valid() {
                return;
            }
            let mut node = dialog_gd.clone();
            let mut bound = node.bind_mut();
            sector_generate_select_planet(&mut bound, &state_for, &name_for_pick);
        });
        button.connect("pressed", &callable);
        grid.add_child(&button);
    }
    let mut close = factory::button(&kit::t("@back").to_string());
    let overlay_for_close: Gd<Control> = overlay_control.clone();
    let callable = Callable::from_fn("sector_picker_close", move |_args: &[&Variant]| {
        let mut overlay = overlay_for_close.clone();
        overlay.queue_free();
    });
    close.connect("pressed", &callable);
    box_.add_child(&close);
}

/// `planet = plan; sector = min(sector, sectors.size - 1); seed = 0`.
fn sector_generate_select_planet(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SectorGenerateState>>,
    planet_name: &str,
) {
    {
        let mut state = state.borrow_mut();
        state.planet = String::from(planet_name);
        let sectors = sector_planet_sector_count(&state.planet);
        if sectors > 0 {
            state.sector = state.sector.min(sectors - 1);
        }
        state.seed = 0;
    }
    if let Some(mut button) = state.borrow().planet_button.clone() {
        button.set_text(&sector_generate_planet_name(state));
    }
    sector_generate_build(dialog, state);
}

/// `valid(text -> planet.sectors.size > parseInt(text, 99999) && ...)`.
fn sector_generate_on_sector_changed(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<SectorGenerateState>>,
    text: &str,
) {
    if !is_valid_int(text) {
        return;
    }
    let value: i64 = text.parse().unwrap_or(0);
    let sectors = sector_planet_sector_count(&state.borrow().planet);
    if value < 0 || (sectors > 0 && value >= sectors) {
        return;
    }
    state.borrow_mut().sector = value;
    sector_generate_refresh_size(state);
    let _ = dialog;
}

/// `_refresh_size`: `editor.sector_size(planet, sector)` into `[ NxN ]`.
fn sector_generate_refresh_size(state: &Rc<RefCell<SectorGenerateState>>) {
    let Some(mut label) = state.borrow().size_label.clone() else {
        return;
    };
    let (planet, sector) = {
        let state = state.borrow();
        (state.planet.clone(), state.sector)
    };
    let size = editor_call(
        "sector_size",
        &[
            GString::from(planet.as_str()).to_variant(),
            sector.to_variant(),
        ],
    )
    .map(|value| to_i64_lenient(&value))
    .unwrap_or(0);
    label.set_text(&GString::from(format!("[ {size}x{size} ]").as_str()));
}

/// `_apply`: `ui.loadAnd(() -> { apply(); hide(); })`.
fn sector_generate_apply(dialog: &Gd<MindDialog>, state: &Rc<RefCell<SectorGenerateState>>) {
    let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot") else {
        sector_generate_apply_now(dialog, state);
        return;
    };
    if !root.has_method("load_and") {
        sector_generate_apply_now(dialog, state);
        return;
    }
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("sector_generate_now", move |_args: &[&Variant]| {
        sector_generate_apply_now(&dialog_for, &state_for);
        Variant::nil()
    });
    let _ = root.call("load_and", &[callable.to_variant()]);
}

/// `_apply_now`: `editor.sector_generate(planet, sector, seed)`, then close.
fn sector_generate_apply_now(dialog: &Gd<MindDialog>, state: &Rc<RefCell<SectorGenerateState>>) {
    let (planet, sector, seed) = {
        let state = state.borrow();
        (state.planet.clone(), state.sector, state.seed)
    };
    let generated = editor_call(
        "sector_generate",
        &[
            GString::from(planet.as_str()).to_variant(),
            sector.to_variant(),
            seed.to_variant(),
        ],
    )
    .map(|value| to_bool_lenient(&value))
    .unwrap_or(false);
    if !generated {
        notify_key("@editor.sectorgenerate");
        return;
    }
    if dialog.is_instance_valid() {
        let mut node = dialog.clone();
        node.bind_mut().close();
    }
}

/// `_planet_sector_count`: `{name, sectors}` from `sector_planets_json`.
fn sector_planet_sector_count(planet: &str) -> i64 {
    let Some(value) = editor_call("sector_planets_json", &[]) else {
        return 0;
    };
    let planets = parse_array(&value.to::<GString>().to_string());
    for row in planets.iter_shared() {
        let Ok(row) = row.try_to::<VarDictionary>() else {
            continue;
        };
        if dict_text_or(&row, "name") == planet {
            return dict_i64_or(&row, "sectors", 0);
        }
    }
    0
}
