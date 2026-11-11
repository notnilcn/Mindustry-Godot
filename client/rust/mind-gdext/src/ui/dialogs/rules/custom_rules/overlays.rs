// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The in-dialog overlays of the custom rules dialog (`@edit`, `@configure`, `@banned*`, `@rules.weather`).
//! Upstream: ui/dialogs/CustomRulesDialog.java (the `_overlay` panels)

use super::*;

/// `@configure`: the loadout editor (`LoadoutDialog.show(999999, rules.loadout,
/// i -> true, reset -> copper x100)`).
pub(super) fn open_loadout(dialog: &mut MindDialog, state: &Rc<RefCell<CustomRulesState>>) {
    let mut panel = overlay_open(dialog, state, "@configure", 420.0);
    let rules = state.borrow().rules.clone();
    let stacks: VarArray = rules
        .borrow()
        .get("loadout")
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default();
    for stack in stacks.iter_shared() {
        let Ok(stack) = stack.try_to::<VarDictionary>() else {
            continue;
        };
        let stack = Rc::new(RefCell::new(stack));
        // code-instantiated: one amount row per loadout stack.
        let mut row = factory::table();
        let item = dict_at_text(&stack, "item");
        let label = factory::label(&item);
        row.bind_mut().add(&label).left().pad_right(6.0);
        let amount = dict_at_i64(&stack, "amount", 0);
        let mut field = factory::field(&amount.to_string());
        let mut minimum = field.get_custom_minimum_size();
        minimum.x = 120.0;
        field.set_custom_minimum_size(minimum);
        {
            let stack_for = stack.clone();
            let field_for = field.clone();
            let callable = Callable::from_fn("rules_loadout_amount", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| (*value).to::<GString>().to_string())
                    .unwrap_or_default();
                let trimmed = text.trim().to_string();
                if !parse_number(&trimmed, false)
                    || trimmed.parse::<f64>().unwrap_or(0.0) > 999_999.0
                {
                    invalid_field(&field_for, true);
                    return;
                }
                invalid_field(&field_for, false);
                let value = trimmed.parse::<f64>().unwrap_or(0.0) as i64;
                stack_for.borrow_mut().set("amount", &value.to_variant());
            });
            field.connect("text_changed", &callable);
        }
        row.bind_mut().add(&field).left();
        panel.bind_mut().add(&row).left().grow_x().pad(2.0);
        panel.bind_mut().row();
    }
    let mut reset_button = wide_button("@settings.reset", "refresh", None, 300.0);
    {
        let rules_for = rules.clone();
        let state_for = state.clone();
        let target = dialog.to_gd();
        let callable = Callable::from_fn("rules_loadout_reset", move |_args: &[&Variant]| {
            let mut entry = VarDictionary::new();
            entry.set("item", &GString::from("copper").to_variant());
            entry.set("amount", &100i64.to_variant());
            let mut loadout = VarArray::new();
            loadout.push(&entry.to_variant());
            rules_for.borrow_mut().set("loadout", &loadout.to_variant());
            close_overlay(&state_for);
            let state = state_for.clone();
            defer_dialog_action("custom_rules_loadout_reset", &target, move |dialog| {
                setup_main(dialog, &state);
            });
        });
        reset_button.connect("pressed", &callable);
    }
    panel.bind_mut().add(&reset_button).left();
    panel.bind_mut().row();
    finish_overlay(&mut panel);
}

/// `_open_banned_blocks`/`_open_banned_units` (`CustomRulesDialog.java:189,210`):
/// the two-pane `BannedContentDialog` edits the working `_rules` copy; edits come
/// back through `banned_edited`.
pub(super) fn open_banned(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<CustomRulesState>>,
    kind: &str,
) {
    let rules = state.borrow().rules.clone();
    let mut context = VarDictionary::new();
    context.set("kind", &GString::from(kind).to_variant());
    context.set("rules", &rules.borrow().to_variant());
    dialog.open_dialog("banned_content", &json_stringify(&context));
    let Some(mut banned) = dialog_node("banned_content") else {
        return;
    };
    if !banned.has_signal("banned_edited") {
        return;
    }
    let mut node = dialog.to_gd();
    if node.has_meta("_batch_i_banned_hook") {
        return;
    }
    let rules_for = rules.clone();
    let callable = Callable::from_fn("custom_rules_banned_edited", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let parsed = Json::parse_string(&GString::from(text.as_str()));
        if let Ok(parsed) = parsed.try_to::<VarDictionary>() {
            *rules_for.borrow_mut() = parsed;
        }
    });
    banned.connect("banned_edited", &callable);
    node.set_meta("_batch_i_banned_hook", &true.to_variant());
}

/// `@rules.weather`: weather-entry editor (`CustomRulesDialog.weatherDialog`).
pub(super) fn open_weather(dialog: &mut MindDialog, state: &Rc<RefCell<CustomRulesState>>) {
    let mut panel = overlay_open(dialog, state, "@rules.weather", 460.0);
    let rules = state.borrow().rules.clone();
    let raw: VarArray = rules
        .borrow()
        .get("weather")
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default();
    // Drop `null` weather names (`rules.weather.removeAll(w -> w.weather == null)`).
    let mut entries = VarArray::new();
    for entry in raw.iter_shared() {
        let Ok(entry) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        if dict_text(&entry, "weather").is_empty() {
            continue;
        }
        entries.push(&entry.to_variant());
    }
    rules.borrow_mut().set("weather", &entries.to_variant());
    let entries = Rc::new(RefCell::new(entries));
    let count = entries.borrow().len();
    for index in 0..count {
        let Some(entry_variant) = entries.borrow().get(index) else {
            continue;
        };
        let Ok(entry) = entry_variant.try_to::<VarDictionary>() else {
            continue;
        };
        let entry = Rc::new(RefCell::new(entry));
        // code-instantiated: one weather entry block (duration/frequency minutes).
        let mut block = factory::table();
        let mut header = factory::table();
        let name = dict_at_text(&entry, "weather");
        let label = factory::label(&name);
        header.bind_mut().add(&label).left().pad_right(6.0);
        let mut remove_entry = wide_button("@cancel", "cancel", None, 300.0);
        {
            let entries_for = entries.clone();
            let entry_for = entry.clone();
            let rules_for = rules.clone();
            let state_for = state.clone();
            let target = dialog.to_gd();
            let callable =
                Callable::from_fn("rules_weather_remove", move |_args: &[&Variant]| {
                    entries_for
                        .borrow_mut()
                        .erase(&entry_for.borrow().to_variant());
                    rules_for
                        .borrow_mut()
                        .set("weather", &entries_for.borrow().to_variant());
                    close_overlay(&state_for);
                    let state = state_for.clone();
                    defer_dialog_action("custom_rules_weather_remove", &target, move |dialog| {
                        open_weather(dialog, &state);
                    });
                });
            remove_entry.connect("pressed", &callable);
        }
        header.bind_mut().add(&remove_entry).left();
        block.bind_mut().add(&header).left().grow_x();
        block.bind_mut().row();
        let always_value = dict_at(&entry, "always", false);
        let mut duration = factory::table();
        duration
            .bind_mut()
            .add(&factory::label(
                &kit::t("@rules.weather.duration").to_string(),
            ))
            .left()
            .pad_right(4.0);
        duration
            .bind_mut()
            .add(&minutes_field(&entry, "minDuration", !always_value))
            .left()
            .pad_right(4.0);
        duration
            .bind_mut()
            .add(&factory::label(&kit::t("@waves.to").to_string()))
            .left()
            .pad_right(4.0);
        duration
            .bind_mut()
            .add(&minutes_field(&entry, "maxDuration", !always_value))
            .left()
            .pad_right(4.0);
        duration
            .bind_mut()
            .add(&factory::label(&kit::t("@unit.minutes").to_string()))
            .left();
        block.bind_mut().add(&duration).left().grow_x();
        block.bind_mut().row();
        let mut frequency = factory::table();
        frequency
            .bind_mut()
            .add(&factory::label(
                &kit::t("@rules.weather.frequency").to_string(),
            ))
            .left()
            .pad_right(4.0);
        frequency
            .bind_mut()
            .add(&minutes_field(&entry, "minFrequency", !always_value))
            .left()
            .pad_right(4.0);
        frequency
            .bind_mut()
            .add(&factory::label(&kit::t("@waves.to").to_string()))
            .left()
            .pad_right(4.0);
        frequency
            .bind_mut()
            .add(&minutes_field(&entry, "maxFrequency", !always_value))
            .left()
            .pad_right(4.0);
        frequency
            .bind_mut()
            .add(&factory::label(&kit::t("@unit.minutes").to_string()))
            .left();
        block.bind_mut().add(&frequency).left().grow_x();
        block.bind_mut().row();
        // `MindWidgets.check(_t("@rules.weather.always"), always, listener)`.
        let mut check = MindCheck::new_alloc();
        {
            let entry_for = entry.clone();
            let state_for = state.clone();
            let target = dialog.to_gd();
            let callable = Callable::from_fn("rules_weather_always", move |args: &[&Variant]| {
                let value = args
                    .first()
                    .map(|value| (*value).to::<bool>())
                    .unwrap_or(false);
                entry_for.borrow_mut().set("always", &value.to_variant());
                close_overlay(&state_for);
                let state = state_for.clone();
                defer_dialog_action("custom_rules_weather_always", &target, move |dialog| {
                    open_weather(dialog, &state);
                });
            });
            check
                .bind_mut()
                .setup(kit::t("@rules.weather.always"), always_value, callable);
        }
        block.bind_mut().add(&check).left();
        panel.bind_mut().add(&block).left().grow_x().pad(3.0);
        panel.bind_mut().row();
    }
    let mut add_button = wide_button("@add", "add", None, 300.0);
    {
        let state_for = state.clone();
        let target = dialog.to_gd();
        let callable = Callable::from_fn("rules_weather_add", move |_args: &[&Variant]| {
            let state = state_for.clone();
            defer_dialog_action("custom_rules_weather_add", &target, move |dialog| {
                open_weather_add(dialog, &state);
            });
        });
        add_button.connect("pressed", &callable);
    }
    panel.bind_mut().add(&add_button).left();
    panel.bind_mut().row();
    finish_overlay(&mut panel);
}

/// `_open_weather_add`: one button per weather content type.
pub(super) fn open_weather_add(dialog: &mut MindDialog, state: &Rc<RefCell<CustomRulesState>>) {
    let mut panel = overlay_open(dialog, state, "@add", 420.0);
    let names: PackedStringArray = kit::sim_host()
        .filter(|host| host.has_method("content_list"))
        .and_then(|mut host| {
            host.call("content_list", &[GString::from("weather").to_variant()])
                .try_to::<PackedStringArray>()
                .ok()
        })
        .unwrap_or_default();
    let rules = state.borrow().rules.clone();
    let entries = Rc::new(RefCell::new(
        rules
            .borrow()
            .get("weather")
            .and_then(|value| value.try_to::<VarArray>().ok())
            .unwrap_or_default(),
    ));
    let mut index = 0;
    for weather_name in names.as_slice() {
        let weather_name = weather_name.to_string();
        let mut button =
            factory::button(&kit::t(&format!("@weather.{weather_name}.name")).to_string());
        button.set_custom_minimum_size(Vector2::new(140.0, 50.0));
        {
            let entries_for = entries.clone();
            let rules_for = rules.clone();
            let state_for = state.clone();
            let target = dialog.to_gd();
            let name_for = weather_name.clone();
            let callable = Callable::from_fn("rules_weather_new", move |_args: &[&Variant]| {
                let mut entry = VarDictionary::new();
                entry.set("weather", &GString::from(name_for.as_str()).to_variant());
                entry.set("minFrequency", &97200.0f64.to_variant());
                entry.set("maxFrequency", &194400.0f64.to_variant());
                entry.set("minDuration", &16200.0f64.to_variant());
                entry.set("maxDuration", &48600.0f64.to_variant());
                entry.set("cooldown", &0.0f64.to_variant());
                entry.set("intensity", &1.0f64.to_variant());
                entry.set("always", &false.to_variant());
                entries_for.borrow_mut().push(&entry.to_variant());
                rules_for
                    .borrow_mut()
                    .set("weather", &entries_for.borrow().to_variant());
                close_overlay(&state_for);
                let state = state_for.clone();
                defer_dialog_action("custom_rules_weather_new", &target, move |dialog| {
                    open_weather(dialog, &state);
                });
            });
            button.connect("pressed", &callable);
        }
        panel.bind_mut().add(&button).pad(1.0);
        index += 1;
        if index % 2 == 0 {
            panel.bind_mut().row();
        }
    }
    finish_overlay(&mut panel);
}

/// `@edit`: the clipboard menu (`CustomRulesDialog.java:60-108`).
pub(super) fn open_edit_menu(dialog: &mut MindDialog, state: &Rc<RefCell<CustomRulesState>>) {
    let mut panel = overlay_open(dialog, state, "@edit.menu", 320.0);
    let rules = state.borrow().rules.clone();
    let reset = state.borrow().reset.clone();
    let mut copy_button = wide_button("@copy.clipboard", "copy", None, 300.0);
    {
        let rules_for = rules.clone();
        let state_for = state.clone();
        let callable = Callable::from_fn("rules_copy_clipboard", move |_args: &[&Variant]| {
            let mut copy = rules_for.borrow().duplicate_deep();
            copy.remove("spawns");
            DisplayServer::singleton().clipboard_set(&json_stringify(&copy));
            let _ = ui_call("show_info", &[kit::t("@copied").to_variant()]);
            close_overlay(&state_for);
        });
        copy_button.connect("pressed", &callable);
    }
    panel.bind_mut().add(&copy_button).left();
    panel.bind_mut().row();
    let clipboard = DisplayServer::singleton().clipboard_get().to_string();
    let loadable = !clipboard.is_empty() && clipboard.trim().starts_with('{');
    let mut load_button = wide_button("@load.clipboard", "download", None, 300.0);
    {
        let rules_for = rules.clone();
        let state_for = state.clone();
        let target = dialog.to_gd();
        let callable = Callable::from_fn("rules_load_clipboard", move |_args: &[&Variant]| {
            let text = DisplayServer::singleton().clipboard_get().to_string();
            let parsed = Json::parse_string(&GString::from(text.as_str()));
            let Ok(parsed) = parsed.try_to::<VarDictionary>() else {
                show_error("@rules.invaliddata");
                close_overlay(&state_for);
                return;
            };
            let mut loaded = parsed;
            // `newRules.spawns/objectives = rules.spawns/objectives` (map-specific).
            let spawns = rules_for
                .borrow()
                .get("spawns")
                .unwrap_or_else(|| VarArray::new().to_variant());
            loaded.set("spawns", &spawns);
            let objectives = rules_for
                .borrow()
                .get("objectives")
                .unwrap_or_else(|| VarArray::new().to_variant());
            loaded.set("objectives", &objectives);
            replace_rules(&rules_for, &loaded);
            close_overlay(&state_for);
            let state = state_for.clone();
            defer_dialog_action("custom_rules_load_clipboard", &target, move |dialog| {
                setup_main(dialog, &state);
            });
        });
        load_button.connect("pressed", &callable);
    }
    load_button.set_disabled(!loadable);
    panel.bind_mut().add(&load_button).left();
    panel.bind_mut().row();
    let mut reset_menu = wide_button("@settings.reset", "refresh", None, 300.0);
    {
        let rules_for = rules.clone();
        let reset_for = reset.clone();
        let state_for = state.clone();
        let target = dialog.to_gd();
        let callable = Callable::from_fn("rules_reset_menu", move |_args: &[&Variant]| {
            let source = if reset_for.borrow().is_empty() {
                rules_for.borrow().clone()
            } else {
                reset_for.borrow().clone()
            };
            let mut copy = VarDictionary::new();
            for key in source.keys_array().iter_shared() {
                if let Some(value) = source.get(&key) {
                    copy.set(&key, &value);
                }
            }
            replace_rules(&rules_for, &copy);
            close_overlay(&state_for);
            let state = state_for.clone();
            defer_dialog_action("custom_rules_reset_menu", &target, move |dialog| {
                setup_main(dialog, &state);
            });
        });
        reset_menu.connect("pressed", &callable);
    }
    panel.bind_mut().add(&reset_menu).left();
    panel.bind_mut().row();
    finish_overlay(&mut panel);
}

/// `_rules.clear()` + copy every key of `source` (the GDScript in-place replace).
pub(super) fn replace_rules(rules: &Rc<RefCell<VarDictionary>>, source: &VarDictionary) {
    let mut target = rules.borrow_mut();
    target.clear();
    for key in source.keys_array().iter_shared() {
        if let Some(value) = source.get(&key) {
            target.set(&key, &value);
        }
    }
}

/// `_show_error(key)`: `ui.showText("@error.title", _t(key))`.
pub(super) fn show_error(key: &str) {
    let _ = ui_call(
        "show_text",
        &[
            kit::t("@error.title").to_variant(),
            kit::t(key).to_variant(),
        ],
    );
}

/// Places a freshly built overlay's cells immediately (a new table is not sorted
/// until the next layout pass).
pub(super) fn finish_overlay(panel: &mut Gd<MindTable>) {
    panel.bind_mut().sort_now();
    if let Some(parent) = panel.get_parent()
        && let Ok(mut container) = parent.try_cast::<godot::classes::Container>()
    {
        container.queue_sort();
    }
}

/// Opens a code-instantiated overlay panel (`BaseDialog` sub-dialogs upstream).
pub(super) fn overlay_open(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<CustomRulesState>>,
    title: &str,
    width: f32,
) -> Gd<MindTable> {
    close_overlay(state);
    // code-instantiated: per-action popup panels have no static scene.
    let mut root = ColorRect::new_alloc();
    // Deterministic handle for the headless sweep (`dialog_sweep_check`) and the
    // PAR-0470 twin walk: the overlay is the only direct `rules_overlay` child.
    root.set_name("rules_overlay");
    root.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.8));
    root.set_anchors_preset(LayoutPreset::FULL_RECT);
    root.set_mouse_filter(MouseFilter::STOP);
    let mut center = CenterContainer::new_alloc();
    center.set_anchors_preset(LayoutPreset::FULL_RECT);
    center.set_mouse_filter(MouseFilter::IGNORE);
    let mut panel_root = PanelContainer::new_alloc();
    panel_root.set_theme_type_variation("defaultDialog");
    panel_root.set_custom_minimum_size(Vector2::new(width, 0.0));
    let mut layout = factory::vbox(6);
    let title_label = factory::styled_label(
        &kit::markup_text(&kit::t(title).to_string()).to_string(),
        "techLabel",
    );
    layout.add_child(&title_label);
    let mut panel_body = factory::table();
    panel_body.set_h_size_flags(SizeFlags::EXPAND_FILL);
    layout.add_child(&panel_body);
    let mut close = factory::button(&kit::t("@back").to_string());
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("rules_overlay_close", move |_args: &[&Variant]| {
            close_overlay(&state_for);
        });
        close.connect("pressed", &callable);
    }
    layout.add_child(&close);
    panel_root.add_child(&layout);
    center.add_child(&panel_root);
    root.add_child(&center);
    let root_control: Gd<Control> = root.upcast();
    let control_for_meta = root_control.clone();
    dialog.base_mut().add_child(&root_control);
    state.borrow_mut().overlay = Some(control_for_meta);
    panel_body
}

/// `_close_overlay`.
pub(super) fn close_overlay(state: &Rc<RefCell<CustomRulesState>>) {
    let overlay = state.borrow_mut().overlay.take();
    if let Some(mut overlay) = overlay
        && overlay.is_instance_valid()
    {
        overlay.queue_free();
    }
}
