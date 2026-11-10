// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `select` support code.
//! Upstream: logic/LStatement.java

use super::*;

// ---------------------------------------------------------------------------
// select popups (`LStatement.showSelect`)
// ---------------------------------------------------------------------------

/// `showSelect`: a popup of `logicTogglet` buttons (`LStatement.java:233-253`).
pub(super) fn open_select(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    statement_name: &str,
) {
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("editor_options") {
        return;
    }
    let options = logic.call(
        "editor_options",
        &[gs(statement_name).to_variant(), field_index.to_variant()],
    );
    let Ok(options) = options.try_to::<VarArray>() else {
        return;
    };
    let dialog_gd = dialog.to_gd();
    // code-instantiated: one transient enum-select popup per open.
    let mut popup = PopupPanel::new_alloc();
    dialog.base_mut().add_child(&popup);
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(select_columns(statement_name, field_index));
    popup.add_child(&grid);
    let unit_actions = unit_actions_enabled();
    let cell_size = select_cell_size(statement_name, field_index);
    let current = current_field_value(state, index, field_index);
    for option in options.iter_shared() {
        let Ok(option) = option.try_to::<VarDictionary>() else {
            continue;
        };
        let option_name = dict_string(&option, "name");
        // `UnitControlStatement` hides build/deconstruct when the rules disable them.
        if statement_name == "ucontrol"
            && !unit_actions
            && (option_name == "build" || option_name == "deconstruct")
        {
            continue;
        }
        // code-instantiated: one toggle per enum option.
        let mut toggle = Button::new_alloc();
        toggle.set_text(&gs(option_text(&option)));
        toggle.set_toggle_mode(true);
        toggle.set_theme_type_variation("logicTogglet");
        toggle.set_pressed(option_name == current);
        toggle.set_custom_minimum_size(cell_size);
        toggle.set_tooltip_text(&gs(select_tooltip(statement_name, field_index, &option)));
        {
            let dialog_for_select = dialog_gd.clone();
            let state_for_select = state.clone();
            let selected = option_name.clone();
            let popup_for_select = popup.clone();
            let callable = Callable::from_fn("logic_select", move |_args: &[&Variant]| {
                let mut popup = popup_for_select.clone();
                popup.hide();
                popup.queue_free();
                if !dialog_for_select.is_instance_valid() {
                    return Variant::nil();
                }
                let mut handle = dialog_for_select.clone();
                let mut bound = handle.bind_mut();
                select_enum(&mut bound, &state_for_select, index, field_index, &selected);
                Variant::nil()
            });
            let mut node: Gd<Node> = toggle.clone().upcast();
            node.connect("pressed", &callable);
        }
        grid.add_child(&toggle);
    }
    connect_popup_free(&popup);
    popup.popup_centered();
}

/// Frees a transient popup when it hides.
pub(super) fn connect_popup_free(popup: &Gd<PopupPanel>) {
    let popup_for_hide = popup.clone();
    let callable = Callable::from_fn("logic_popup_free", move |_args: &[&Variant]| {
        if popup_for_hide.is_instance_valid() {
            let mut popup = popup_for_hide.clone();
            popup.queue_free();
        }
        Variant::nil()
    });
    let mut popup = popup.clone();
    popup.connect("popup_hide", &callable);
}

/// `fieldAlignSelect` (`LStatement.java:259-268`): the nine align presets, set as
/// `@<name>` (`LStatement.afterRead` normalizes them).
pub(super) fn open_align_select(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let dialog_gd = dialog.to_gd();
    // code-instantiated: one transient align-select popup per open.
    let mut popup = PopupPanel::new_alloc();
    dialog.base_mut().add_child(&popup);
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(3);
    popup.add_child(&grid);
    let current = current_field_value(state, index, field_index);
    let aligns = [
        "topLeft",
        "top",
        "topRight",
        "left",
        "center",
        "right",
        "bottomLeft",
        "bottom",
        "bottomRight",
    ];
    for align in aligns {
        // code-instantiated: one align toggle per preset.
        let mut toggle = Button::new_alloc();
        toggle.set_text(&gs(field_label_text(align)));
        toggle.set_toggle_mode(true);
        toggle.set_theme_type_variation("logicTogglet");
        toggle.set_pressed(current == format!("@{align}"));
        toggle.set_custom_minimum_size(Vector2::new(150.0, 40.0));
        {
            let dialog_for_select = dialog_gd.clone();
            let state_for_select = state.clone();
            let value = format!("@{align}");
            let popup_for_select = popup.clone();
            let callable = Callable::from_fn("logic_align", move |_args: &[&Variant]| {
                let mut popup = popup_for_select.clone();
                popup.hide();
                popup.queue_free();
                if !dialog_for_select.is_instance_valid() {
                    return Variant::nil();
                }
                let mut handle = dialog_for_select.clone();
                let mut bound = handle.bind_mut();
                select_enum(&mut bound, &state_for_select, index, field_index, &value);
                Variant::nil()
            });
            let mut node: Gd<Node> = toggle.clone().upcast();
            node.connect("pressed", &callable);
        }
        grid.add_child(&toggle);
    }
    connect_popup_free(&popup);
    popup.popup_centered();
}

/// `PrintCharStatement`'s pencil grid (`LStatements.java:290-311`).
pub(super) fn open_char_select(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let dialog_gd = dialog.to_gd();
    // code-instantiated: one transient character-picker popup per open.
    let mut popup = PopupPanel::new_alloc();
    dialog.base_mut().add_child(&popup);
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(8);
    popup.add_child(&grid);
    for code in 32u32..127u32 {
        let Some(ch) = char::from_u32(code) else {
            continue;
        };
        // code-instantiated: one character toggle per printable code point.
        let mut toggle = Button::new_alloc();
        toggle.set_text(&gs(ch.to_string().as_str()));
        toggle.set_theme_type_variation("flatt");
        toggle.set_custom_minimum_size(Vector2::new(32.0, 32.0));
        {
            let dialog_for_select = dialog_gd.clone();
            let state_for_select = state.clone();
            let value = code.to_string();
            let popup_for_select = popup.clone();
            let callable = Callable::from_fn("logic_char", move |_args: &[&Variant]| {
                let mut popup = popup_for_select.clone();
                popup.hide();
                popup.queue_free();
                if !dialog_for_select.is_instance_valid() {
                    return Variant::nil();
                }
                let mut handle = dialog_for_select.clone();
                let mut bound = handle.bind_mut();
                select_enum(&mut bound, &state_for_select, index, field_index, &value);
                Variant::nil()
            });
            let mut node: Gd<Node> = toggle.clone().upcast();
            node.connect("pressed", &callable);
        }
        grid.add_child(&toggle);
    }
    connect_popup_free(&popup);
    popup.popup_centered();
}

/// A select changed: commit, apply the Java change fixups and rebuild the
/// element (`DrawStatement.rebuild`, `rebuild(table)` in each `build`).
pub(super) fn select_enum(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    option: &str,
) {
    if !commit_field_value(state, option, index, field_index) {
        return;
    }
    let Some(mut logic) = kit::logic() else {
        return;
    };
    let (name, values) = {
        let st = state.borrow();
        let Some(row) = st.statements.get(index.max(0) as usize) else {
            return;
        };
        (row.name.clone(), row.fields.clone())
    };
    if logic.has_method("apply_type_change") {
        let changed = logic.call(
            "apply_type_change",
            &[
                gs(name.as_str()).to_variant(),
                packed_strings(&values).to_variant(),
            ],
        );
        if let Some(new_values) = packed_to_vec(&changed)
            && let Some(row) = state.borrow_mut().statements.get_mut(index.max(0) as usize)
        {
            row.fields = new_values;
        }
    }
    state.borrow_mut().modified = true;
    rebuild_list(dialog, state);
}

/// `col(table, x, res -> ...)`: the colour picker commits `%rrggbb[aa]`.
pub(super) fn set_color(
    state: &Rc<RefCell<LogicState>>,
    color: Color,
    index: i64,
    field_index: i64,
) {
    let alpha = color.a >= 1.0;
    let mut packed = format!(
        "{:02x}{:02x}{:02x}",
        (color.r * 255.0) as i64,
        (color.g * 255.0) as i64,
        (color.b * 255.0) as i64
    );
    if !alpha {
        packed.push_str(&format!("{:02x}", (color.a * 255.0) as i64));
    }
    commit_field_value(state, &format!("%{packed}"), index, field_index);
}

pub(super) fn parse_color(value: &str) -> Color {
    let text = value.strip_prefix('%').unwrap_or(value);
    if text.len() < 6 {
        return Color::WHITE;
    }
    let component = |range: std::ops::Range<usize>| -> u8 {
        text.get(range)
            .and_then(|slice| u8::from_str_radix(slice, 16).ok())
            .unwrap_or_default()
    };
    let r = component(0..2);
    let g = component(2..4);
    let b = component(4..6);
    let a = if text.len() >= 8 {
        component(6..8)
    } else {
        255
    };
    Color::from_rgba8(r, g, b, a)
}

/// `_unit_actions_enabled`: the `logicUnitBuild`/`logicUnitDeconstruct` rules
/// probe (`UnitControlStatement.build`).
pub(super) fn unit_actions_enabled() -> bool {
    let Some(mut host) = kit::sim_host() else {
        return true;
    };
    if !host.has_method("logic_rules_probe") {
        return true;
    }
    let probe = host.call("logic_rules_probe", &[]);
    let text = probe
        .try_to::<GString>()
        .map(|value| value.to_string())
        .unwrap_or_default();
    let Ok(dict) = Json::parse_string(&gs(text.as_str())).try_to::<VarDictionary>() else {
        return true;
    };
    dict_bool(&dict, "logicUnitBuild", true) || dict_bool(&dict, "logicUnitDeconstruct", true)
}
