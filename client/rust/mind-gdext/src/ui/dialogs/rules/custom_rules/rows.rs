// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The rule-row builders of the custom rules dialog (conditions, `check`/`number`/`text` rows).
//! Upstream: ui/dialogs/CustomRulesDialog.java (the row builders)

use super::*;

/// `Callable()`: a row with no condition is always enabled.
pub(super) fn always() -> Cond {
    Rc::new(|| true)
}

/// `_rules.get(key, default)` as a condition.
pub(super) fn flag(dict: &Rc<RefCell<VarDictionary>>, key: &'static str, default: bool) -> Cond {
    let dict = dict.clone();
    Rc::new(move || dict_at(&dict, key, default))
}

/// `not condition`.
pub(super) fn not(cond: Cond) -> Cond {
    Rc::new(move || !cond())
}

/// `a and b`.
pub(super) fn and(first: Cond, second: Cond) -> Cond {
    Rc::new(move || first() && second())
}

/// `_in_game()` as a condition.
pub(super) fn game_cond() -> Cond {
    Rc::new(in_game)
}

/// One `number`/`numberi` row value (`_number` cons receives a float,
/// `_numberi` cons receives an int).
#[derive(Clone, Copy)]
pub(super) enum NumberValue {
    Int(i64),
    Float(f64),
}

/// `_check`: one check row into the current category.
pub(super) fn check_rule(
    state: &Rc<RefCell<CustomRulesState>>,
    dict: &Rc<RefCell<VarDictionary>>,
    key: &str,
    field: &'static str,
    default: bool,
    cond: Cond,
) {
    let checked = dict_at(dict, field, default);
    let dict_cons = dict.clone();
    let on_change = Rc::new(move |value: bool| {
        set_at(&dict_cons, field, value.to_variant());
    });
    if let Some(row) = build_check_row(state, key, checked, on_change, cond) {
        add_current(state, &row, false);
    }
}

/// `_check_in`: one check row into a plain container (the team collapsers).
pub(super) fn check_rule_in(
    state: &Rc<RefCell<CustomRulesState>>,
    target: &Gd<Control>,
    dict: &Rc<RefCell<VarDictionary>>,
    key: &str,
    field: &'static str,
    default: bool,
    cond: Cond,
) {
    let checked = dict_at(dict, field, default);
    let dict_cons = dict.clone();
    let on_change = Rc::new(move |value: bool| {
        set_at(&dict_cons, field, value.to_variant());
    });
    if let Some(row) = build_check_row(state, key, checked, on_change, cond) {
        add_in(target, &row, false);
    }
}

/// The team collapser's multiplier rows: `_check_in` with a `func(v: float)`
/// consumer, so the toggle stores `1.0`/`0.0` in the multiplier field.
pub(super) fn check_num_in(
    state: &Rc<RefCell<CustomRulesState>>,
    target: &Gd<Control>,
    dict: &Rc<RefCell<VarDictionary>>,
    key: &str,
    field: &'static str,
    default: f64,
    cond: Cond,
) {
    let checked = dict_at_f64(dict, field, default) != 0.0;
    let dict_cons = dict.clone();
    let on_change = Rc::new(move |value: bool| {
        set_at(
            &dict_cons,
            field,
            (if value { 1.0 } else { 0.0 }).to_variant(),
        );
    });
    if let Some(row) = build_check_row(state, key, checked, on_change, cond) {
        add_in(target, &row, false);
    }
}

/// The `cons` of one `number`/`numberi` row: write `value * scale` back.
pub(super) fn number_cons(
    dict: &Rc<RefCell<VarDictionary>>,
    field: &'static str,
    scale: f64,
    integer: bool,
    store_float: bool,
) -> Rc<dyn Fn(NumberValue)> {
    let dict = dict.clone();
    Rc::new(move |value: NumberValue| {
        let stored = match value {
            NumberValue::Int(number) => {
                if integer && !store_float {
                    (((number as f64) * scale) as i64).to_variant()
                } else {
                    ((number as f64) * scale).to_variant()
                }
            }
            NumberValue::Float(number) => (number * scale).to_variant(),
        };
        set_at(&dict, field, stored);
    })
}

/// `_number`/`_numberi`: one number row into the current category.
pub(super) fn number_rule(
    state: &Rc<RefCell<CustomRulesState>>,
    dict: &Rc<RefCell<VarDictionary>>,
    spec: NumberSpec,
    cond: Cond,
) {
    let raw = dict_at_f64(dict, spec.field, spec.default);
    let shown = (raw / spec.div).min(spec.cap);
    let on_change = number_cons(dict, spec.field, spec.scale, spec.integer, spec.store_float);
    if let Some(row) = build_number_row(
        state,
        spec.key,
        spec.integer,
        shown,
        spec.minimum,
        spec.maximum,
        on_change,
        cond,
    ) {
        add_current(state, &row, true);
    }
}

/// `_number_in`/`_numberi_in`: one number row into a plain container.
pub(super) fn number_rule_in(
    state: &Rc<RefCell<CustomRulesState>>,
    target: &Gd<Control>,
    dict: &Rc<RefCell<VarDictionary>>,
    spec: NumberSpec,
    cond: Cond,
) {
    let raw = dict_at_f64(dict, spec.field, spec.default);
    let shown = (raw / spec.div).min(spec.cap);
    let on_change = number_cons(dict, spec.field, spec.scale, spec.integer, spec.store_float);
    if let Some(row) = build_number_row(
        state,
        spec.key,
        spec.integer,
        shown,
        spec.minimum,
        spec.maximum,
        on_change,
        cond,
    ) {
        add_in(target, &row, true);
    }
}

/// `_text`: one text row (the music track lists).
pub(super) fn text_rule(
    state: &Rc<RefCell<CustomRulesState>>,
    dict: &Rc<RefCell<VarDictionary>>,
    key: &str,
    field: &'static str,
    valid: fn(&str) -> bool,
    cond: Cond,
) {
    let value = dict
        .borrow()
        .get(field)
        .map(|value| music_text(&value))
        .unwrap_or_default();
    let dict_cons = dict.clone();
    let on_change = Rc::new(move |text: String| {
        set_at(&dict_cons, field, music_value(&text));
    });
    if let Some(row) = build_text_row(state, key, &value, Rc::new(valid), on_change, cond) {
        add_current(state, &row, true);
    }
}

/// `_register_row` + the first `apply`.
pub(super) fn register_row(
    state: &Rc<RefCell<CustomRulesState>>,
    cond: Cond,
    apply: Rc<dyn Fn(bool)>,
) {
    let enabled = cond();
    apply(enabled);
    state.borrow_mut().rows.push(RuleRow { cond, apply });
}

/// Re-evaluates every row condition (Java does this each frame; the port runs it
/// on every edit and rebuild).
pub(super) fn refresh_conditions(state: &Rc<RefCell<CustomRulesState>>) {
    let rows: Vec<RowEntry> = state
        .borrow()
        .rows
        .iter()
        .map(|row| (row.cond.clone(), row.apply.clone()))
        .collect();
    for (cond, apply) in rows {
        apply(cond());
    }
}

/// One `check` row (`check(text, cons, prov, condition)` + `ruleInfo`).
pub(super) fn build_check_row(
    state: &Rc<RefCell<CustomRulesState>>,
    key: &str,
    checked: bool,
    on_change: Rc<dyn Fn(bool)>,
    cond: Cond,
) -> Option<Gd<MindTable>> {
    if !matches_key(state, key) {
        return None;
    }
    let mut row = factory::table();
    // code-instantiated: one themed check per rules row (`Elems.check`).
    let mut check = MindCheck::new_alloc();
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("rules_check_toggle", move |args: &[&Variant]| {
            let value = args
                .first()
                .map(|value| (*value).to::<bool>())
                .unwrap_or(false);
            on_change(value);
            refresh_conditions(&state_for);
        });
        check
            .bind_mut()
            .setup(GString::from(plain(key).as_str()), checked, callable);
    }
    // `current.check(text, cons)` is a plain `CheckBox` (`Styles.defaultCheck`,
    // no background) left-aligned at its own width (`CustomRulesDialog.java:
    // 478-479`), not `Elems.check`'s grayPanel strip.
    for style in [
        "normal",
        "hover",
        "pressed",
        "hover_pressed",
        "disabled",
        "focus",
    ] {
        let empty = StyleBoxEmpty::new_gd();
        check.add_theme_stylebox_override(style, &empty);
    }
    check.bind_mut().set_content_inset(0.0, 0.0);
    {
        let mut control: Gd<Control> = check.clone().upcast();
        rule_info(&mut control, key);
    }
    row.bind_mut().add(&check).left();
    let check_for_apply = check.clone();
    register_row(
        state,
        cond,
        Rc::new(move |enabled: bool| {
            let mut button: Gd<Button> = check_for_apply.clone().upcast();
            button.set_disabled(!enabled);
            button.set_modulate(if enabled {
                Color::from_rgba(1.0, 1.0, 1.0, 1.0)
            } else {
                Color::from_rgba(1.0, 1.0, 1.0, 0.6)
            });
        }),
    );
    Some(row)
}

/// One `number`/`numberi` row (width-120 field, min/max validated).
///
/// The argument list mirrors `_make_number_row(key, integer, cons, prov, cond,
/// min, max)`; the value/cons pair replaces the two GDScript callables.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_number_row(
    state: &Rc<RefCell<CustomRulesState>>,
    key: &str,
    integer: bool,
    value: f64,
    minimum: f64,
    maximum: f64,
    on_change: Rc<dyn Fn(NumberValue)>,
    cond: Cond,
) -> Option<Gd<MindTable>> {
    if !matches_key(state, key) {
        return None;
    }
    let mut row = factory::table();
    let label = factory::label(&kit::tm(key).to_string());
    {
        let mut control: Gd<Control> = label.clone().upcast();
        rule_info(&mut control, key);
    }
    row.bind_mut().add(&label).left().pad_right(5.0);
    let mut field = factory::field(&number_text(value, integer));
    let mut minimum_size = field.get_custom_minimum_size();
    minimum_size.x = 120.0;
    field.set_custom_minimum_size(minimum_size);
    field.set_h_size_flags(SizeFlags::SHRINK_BEGIN);
    {
        let state_for = state.clone();
        let field_for = field.clone();
        let callable = Callable::from_fn("rules_number_changed", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            let trimmed = text.trim().to_string();
            if !parse_number(&trimmed, integer) {
                invalid_field(&field_for, true);
                return;
            }
            let parsed = trimmed.parse::<f64>().unwrap_or(0.0);
            if parsed < minimum || parsed > maximum {
                invalid_field(&field_for, true);
                return;
            }
            invalid_field(&field_for, false);
            if integer {
                on_change(NumberValue::Int(parsed as i64));
            } else {
                on_change(NumberValue::Float(parsed));
            }
            refresh_conditions(&state_for);
        });
        field.connect("text_changed", &callable);
    }
    row.bind_mut()
        .add(&field)
        .left()
        .pad_right(if integer { 100.0 } else { 50.0 });
    let field_for_apply = field.clone();
    let label_for_apply = label.clone();
    register_row(
        state,
        cond,
        Rc::new(move |enabled: bool| {
            let mut field = field_for_apply.clone();
            field.set_editable(enabled);
            let mut label = rich(&label_for_apply);
            label.add_theme_color_override(
                "default_color",
                if enabled {
                    Color::WHITE
                } else {
                    Color::from_rgba(0.6, 0.6, 0.6, 1.0)
                },
            );
        }),
    );
    Some(row)
}

/// One `text` row (width-300 field, `valid` predicate).
pub(super) fn build_text_row(
    state: &Rc<RefCell<CustomRulesState>>,
    key: &str,
    value: &str,
    valid: Rc<dyn Fn(&str) -> bool>,
    on_change: Rc<dyn Fn(String)>,
    cond: Cond,
) -> Option<Gd<MindTable>> {
    if !matches_key(state, key) {
        return None;
    }
    let mut row = factory::table();
    let label = factory::label(&kit::tm(key).to_string());
    {
        let mut control: Gd<Control> = label.clone().upcast();
        rule_info(&mut control, key);
    }
    row.bind_mut().add(&label).left().pad_right(5.0);
    let mut field = factory::field(value);
    let mut minimum_size = field.get_custom_minimum_size();
    minimum_size.x = 300.0;
    field.set_custom_minimum_size(minimum_size);
    field.set_h_size_flags(SizeFlags::SHRINK_BEGIN);
    {
        let state_for = state.clone();
        let field_for = field.clone();
        let callable = Callable::from_fn("rules_text_changed", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            let ok = valid(&text);
            invalid_field(&field_for, !ok);
            if !ok {
                return;
            }
            on_change(text);
            refresh_conditions(&state_for);
        });
        field.connect("text_changed", &callable);
    }
    row.bind_mut().add(&field).left().pad_right(50.0);
    let field_for_apply = field.clone();
    let label_for_apply = label.clone();
    register_row(
        state,
        cond,
        Rc::new(move |enabled: bool| {
            let mut field = field_for_apply.clone();
            field.set_editable(enabled);
            let mut label = rich(&label_for_apply);
            label.add_theme_color_override(
                "default_color",
                if enabled {
                    Color::WHITE
                } else {
                    Color::from_rgba(0.6, 0.6, 0.6, 1.0)
                },
            );
        }),
    );
    Some(row)
}
