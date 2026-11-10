// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The planet/environment rules and the weather editor of the custom rules dialog.
//! Upstream: ui/dialogs/CustomRulesDialog.java (planet, ambient-light and weather rows)

use super::*;

/// `@rules.ambientlight` colour picker row (`ui.picker.show`).
pub(super) fn ambient_light_button(state: &Rc<RefCell<CustomRulesState>>) -> Gd<MindTable> {
    let mut row = factory::table();
    // code-instantiated: the swatch reflects `rules.ambientLight`.
    let mut swatch = ColorRect::new_alloc();
    swatch.set_custom_minimum_size(Vector2::new(50.0, 50.0));
    swatch.set_color(ambient_color(state));
    row.bind_mut().add(&swatch).pad(4.0);
    let label = factory::label(&kit::t("@rules.ambientlight").to_string());
    row.bind_mut().add(&label).left().pad_right(10.0);
    let mut button = Button::new_alloc();
    button.set_text("");
    button.set_custom_minimum_size(Vector2::new(250.0, 50.0));
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("rules_ambient", move |_args: &[&Variant]| {
            let rules = state_for.borrow().rules.clone();
            let swatch_for = swatch.clone();
            let listener: Rc<dyn Fn(Color)> = Rc::new(move |color: Color| {
                let hex = format!(
                    "{:02x}{:02x}{:02x}{:02x}",
                    (color.r * 255.0) as i32,
                    (color.g * 255.0) as i32,
                    (color.b * 255.0) as i32,
                    (color.a * 255.0) as i32
                );
                rules
                    .borrow_mut()
                    .set("ambientLight", &GString::from(hex.as_str()).to_variant());
                if !swatch_for.is_instance_valid() {
                    return;
                }
                let mut swatch = swatch_for.clone();
                swatch.set_color(color);
            });
            show_color_picker(&state_for, listener);
        });
        button.connect("pressed", &callable);
    }
    row.bind_mut().add(&button).left();
    row
}

/// `rules.ambientLight` as a colour (`_ambient_color`).
pub(super) fn ambient_color(state: &Rc<RefCell<CustomRulesState>>) -> Color {
    let hex = state
        .borrow()
        .rules
        .borrow()
        .get("ambientLight")
        .map(|value| variant_text(&value))
        .unwrap_or_else(|| String::from("00000000"));
    if hex.len() == 8 {
        let channel = |index: usize| -> u8 {
            hex.get(index..index + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .unwrap_or(0)
        };
        return Color::from_rgba8(channel(0), channel(2), channel(4), channel(6));
    }
    Color::from_rgba(0.0, 0.0, 0.0, 0.0)
}

/// `ui.picker.show(col, col::set)` (`ColorPicker` signal `color_selected`).
pub(super) fn show_color_picker(
    state: &Rc<RefCell<CustomRulesState>>,
    listener: Rc<dyn Fn(Color)>,
) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("open_dialog") {
        return;
    }
    let Some(mut ui_root) = kit::node("/root/Spine/Ui/UiRoot") else {
        return;
    };
    if !ui_root.has_method("dialog") {
        return;
    }
    let _ = ui.call(
        "open_dialog",
        &[
            GString::from("picker").to_variant(),
            GString::new().to_variant(),
        ],
    );
    let Ok(mut node) = ui_root
        .call("dialog", &[GString::from("picker").to_variant()])
        .try_to::<Gd<Node>>()
    else {
        return;
    };
    if !node.has_signal("color_selected") {
        return;
    }
    let color = ambient_color(state);
    state.borrow_mut().picker_listener = Some(listener);
    if node.has_method("set_color") {
        node.call("set_color", &[color.to_variant()]);
    }
    if node.has_meta("_batch_i_picker_hook") {
        return;
    }
    let state_for = state.clone();
    let callable = Callable::from_fn("custom_rules_color_selected", move |args: &[&Variant]| {
        let color = args
            .first()
            .map(|value| (*value).to::<Color>())
            .unwrap_or(Color::WHITE);
        let listener = state_for.borrow().picker_listener.clone();
        if let Some(listener) = listener {
            listener(color);
        }
    });
    node.connect("color_selected", &callable);
    node.set_meta("_batch_i_picker_hook", &true.to_variant());
}

/// `category("planet")`: planet toggle buttons (`planet.applyRules(rules, true)`)
/// plus `@rules.anyenv`.
pub(super) fn planet_table(state: &Rc<RefCell<CustomRulesState>>) -> Gd<MindTable> {
    let mut table = factory::table();
    let rows: Vec<VarDictionary> = campaign_call("rules_planets", &[])
        .and_then(|value| value.try_to::<Array<VarDictionary>>().ok())
        .unwrap_or_default()
        .iter_shared()
        .collect();
    let rules = state.borrow().rules.clone();
    let buttons: PlanetButtons = Rc::new(RefCell::new(Vec::new()));
    let selected_planet = rules
        .borrow()
        .get("planet")
        .map(|value| variant_text(&value))
        .unwrap_or_else(|| String::from("serpulo"));
    let mut index = 0;
    for row in &rows {
        let planet_name = dict_text(row, "name");
        // code-instantiated: one planet toggle per landable planet.
        let mut button = Button::new_alloc();
        button.set_text(&kit::t(&format!("@planet.{planet_name}.name")));
        button.set_theme_type_variation("flatTogglet");
        button.set_toggle_mode(true);
        button.set_pressed_no_signal(selected_planet == planet_name);
        button.set_custom_minimum_size(Vector2::new(140.0, 50.0));
        {
            let name_for = planet_name.clone();
            let rules_for = rules.clone();
            let state_for = state.clone();
            let buttons_for = buttons.clone();
            let callable = Callable::from_fn("rules_planet_pick", move |_args: &[&Variant]| {
                apply_planet(&rules_for, &name_for);
                for (button, name) in buttons_for.borrow().iter() {
                    let mut button = button.clone();
                    button.set_pressed_no_signal(*name == name_for);
                }
                refresh_conditions(&state_for);
            });
            button.connect("pressed", &callable);
        }
        table.bind_mut().add(&button).pad(1.0);
        buttons.borrow_mut().push((button, planet_name));
        index += 1;
        if index % 3 == 0 {
            table.bind_mut().row();
        }
    }
    let mut any = Button::new_alloc();
    any.set_text(&kit::t("@rules.anyenv"));
    any.set_theme_type_variation("flatTogglet");
    any.set_toggle_mode(true);
    any.set_pressed_no_signal(selected_planet == "sun");
    any.set_custom_minimum_size(Vector2::new(140.0, 50.0));
    {
        let rules_for = rules.clone();
        let state_for = state.clone();
        let buttons_for = buttons.clone();
        let any_for = any.clone();
        let callable = Callable::from_fn("rules_anyenv", move |_args: &[&Variant]| {
            rules_for
                .borrow_mut()
                .set("attributes", &VarDictionary::new().to_variant());
            rules_for
                .borrow_mut()
                .set("env", &(default_env() as f64).to_variant());
            rules_for
                .borrow_mut()
                .set("planet", &GString::from("sun").to_variant());
            for (button, _) in buttons_for.borrow().iter() {
                let mut button = button.clone();
                button.set_pressed_no_signal(false);
            }
            let mut any_button: Gd<Button> = any_for.clone();
            any_button.set_pressed_no_signal(true);
            refresh_conditions(&state_for);
        });
        any.connect("pressed", &callable);
    }
    table.bind_mut().add(&any).pad(1.0);
    table
}

/// `planet.applyRules(rules, true)` through the campaign facade.
pub(super) fn apply_planet(rules: &Rc<RefCell<VarDictionary>>, planet_name: &str) {
    let json = json_stringify(&rules.borrow());
    let Some(updated) = campaign_call(
        "apply_planet_rules",
        &[
            GString::from(planet_name).to_variant(),
            GString::from(json.as_str()).to_variant(),
        ],
    ) else {
        return;
    };
    let text = updated.to::<GString>().to_string();
    if text.is_empty() {
        return;
    }
    // `if parsed is Dictionary`: an unparseable reply leaves the rules alone, an
    // empty object clears them (the GDScript replaces the whole dictionary).
    let Ok(parsed) = Json::parse_string(&GString::from(text.as_str())).try_to::<VarDictionary>()
    else {
        return;
    };
    replace_rules(rules, &parsed);
}

/// `_planet_env(planet)` (`MindCampaign.planet_default_env`).
pub(super) fn planet_env(planet_name: &str) -> i64 {
    campaign_call(
        "planet_default_env",
        &[GString::from(planet_name).to_variant()],
    )
    .map(|value| variant_i64(&value))
    .unwrap_or(0)
}

/// `_default_env()` (`MindCampaign.default_env`).
pub(super) fn default_env() -> i64 {
    campaign_call("default_env", &[])
        .map(|value| variant_i64(&value))
        .unwrap_or(233)
}

/// One minutes field of a weather entry (`field(f, entry.minDuration / toMinutes,
/// v -> entry.minDuration = v * toMinutes)`).
pub(super) fn minutes_field(
    entry: &Rc<RefCell<VarDictionary>>,
    key: &'static str,
    enabled: bool,
) -> Gd<LineEdit> {
    let value = dict_at_f64(entry, key, 0.0) / 60.0;
    let mut field = factory::field(&number_text(value, false));
    field.set_custom_minimum_size(Vector2::new(90.0, 40.0));
    field.set_editable(enabled);
    let entry_for = entry.clone();
    let field_for = field.clone();
    let callable = Callable::from_fn("rules_weather_minutes", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let trimmed = text.trim().to_string();
        if !parse_number(&trimmed, false) {
            invalid_field(&field_for, true);
            return;
        }
        invalid_field(&field_for, false);
        let value = trimmed.parse::<f64>().unwrap_or(0.0);
        entry_for
            .borrow_mut()
            .set(key, &(value * 60.0).to_variant());
    });
    field.connect("text_changed", &callable);
    field
}

/// `MusicContainer` list text (`Seq<MusicContainer>.toString(", ")`); the port's
/// JSON shape is `{"tracks": ["name", ...]}`.
pub(super) fn music_text(value: &Variant) -> String {
    if value.get_type() != VariantType::DICTIONARY {
        return String::new();
    }
    let Ok(dict) = value.try_to::<VarDictionary>() else {
        return String::new();
    };
    let Ok(tracks) = dict
        .get("tracks")
        .unwrap_or_else(|| VarArray::new().to_variant())
        .try_to::<VarArray>()
    else {
        return String::new();
    };
    tracks
        .iter_shared()
        .map(|track| variant_text(&track))
        .collect::<Vec<String>>()
        .join(", ")
}

/// `_music_value`: null for an empty list, else `{"tracks": [...]}`.
pub(super) fn music_value(text: &str) -> Variant {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Variant::nil();
    }
    let mut tracks = VarArray::new();
    for part in trimmed.split(',') {
        let name = part.trim();
        if !name.is_empty() {
            tracks.push(&GString::from(name).to_variant());
        }
    }
    let mut dict = VarDictionary::new();
    dict.set("tracks", &tracks.to_variant());
    dict.to_variant()
}

/// `MusicContainer::new` accepts any comma-separated identifier list.
pub(super) fn music_valid(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    for part in trimmed.split(',') {
        let name = part.trim();
        if name.is_empty() || !is_valid_identifier(name) {
            return false;
        }
    }
    true
}
