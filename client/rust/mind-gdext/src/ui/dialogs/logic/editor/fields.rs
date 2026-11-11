// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `fields` support code.
//! Upstream: logic/LStatement.java

use super::*;

// ---------------------------------------------------------------------------
// field row (`LStatement.build(Table)`)
// ---------------------------------------------------------------------------

/// The per-statement field row: `MindLogic.editor_layout` returns the exact Java
/// item order (labels, `=`/`linknum` separators, enum selects, and the
/// colour/align/char pencils). Wrapping statements flow like `WrapTable`; the
/// rest (`jump`) stay on one row.
pub(super) fn field_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    row: &StatementRow,
    color: Color,
    wraps: bool,
) -> Gd<Control> {
    let dialog_gd = dialog.to_gd();
    // code-instantiated: the field row is rebuilt per statement on every list
    // rebuild (`LCanvas.StatementElem` calls `st.build(t)`).
    let mut fields_box: Gd<Container> = if wraps {
        let mut flow = HFlowContainer::new_alloc();
        flow.add_theme_constant_override("h_separation", 4);
        flow.add_theme_constant_override("v_separation", 2);
        flow.upcast()
    } else {
        let hbox = factory::hbox(4);
        hbox.upcast()
    };
    fields_box.set_mouse_filter(MouseFilter::PASS);
    let Some(mut logic) = kit::logic() else {
        return fields_box.upcast();
    };
    let statement_name = row.name.clone();
    let mut values = row.fields.clone();
    // `SetRuleStatement.rebuild` normalizes `p1` to `@sharded` for team rules;
    // Java mutates the statement at build time, so the dialog mirrors it.
    if logic.has_method("editor_normalize") {
        let normalized = logic.call(
            "editor_normalize",
            &[
                gs(statement_name.as_str()).to_variant(),
                packed_strings(&values).to_variant(),
            ],
        );
        if let Some(changed) = packed_to_vec(&normalized)
            && changed != values
        {
            values = changed;
            if let Some(row) = state.borrow_mut().statements.get_mut(index.max(0) as usize) {
                row.fields = values.clone();
            }
            state.borrow_mut().modified = true;
        }
    }
    if !logic.has_method("editor_layout") {
        return fields_box.upcast();
    }
    let items = logic.call(
        "editor_layout",
        &[
            gs(statement_name.as_str()).to_variant(),
            packed_strings(&values).to_variant(),
        ],
    );
    let Ok(items) = items.try_to::<VarArray>() else {
        return fields_box.upcast();
    };
    let items = filter_layout(&statement_name, &values, items);
    for entry in items.iter_shared() {
        let Ok(entry) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        let kind = dict_string(&entry, "kind");
        let field_index = entry
            .get("index")
            .and_then(|value| value.try_to::<i64>().ok())
            .unwrap_or(-1);
        let label = field_label_text(&dict_string(&entry, "label"));
        let label_before = dict_bool(&entry, "label_before", true);
        let value = if field_index >= 0 && (field_index as usize) < values.len() {
            values[field_index as usize].clone()
        } else {
            String::new()
        };
        match kind.as_str() {
            "text" => {
                if !label.is_empty() {
                    add_any(&fields_box, &field_label(&label));
                }
            }
            "field" => {
                if label_before && !label.is_empty() {
                    add_any(&fields_box, &field_label(&label));
                }
                // `LStatement.field`: `Styles.nodeField`, `size(180f, 40f)`,
                // tinted with the element colour; `print`/`format` grow.
                // code-instantiated: one text field per statement field.
                let mut edit = LineEdit::new_alloc();
                edit.set_text(&gs(value.as_str()));
                let mut width = 180.0_f32;
                // `ApplyStatusStatement`'s effect field is `.width(240f)`
                // (`LStatements.java:1447`).
                if statement_name == "status" && field_index == 1 {
                    width = 240.0;
                }
                edit.set_custom_minimum_size(Vector2::new(width, 40.0));
                if dict_bool(&entry, "wide", false) {
                    edit.set_h_size_flags(SizeFlags::EXPAND_FILL);
                }
                apply_node_field(&mut edit, color);
                edit.set_tooltip_text(&gs(bundle_tooltip(&format!(
                    "{}.{}",
                    insert_spaces(&type_name(state, &statement_name)),
                    label
                ))));
                on_signal(
                    &edit.clone().upcast(),
                    "text_submitted",
                    &dialog_gd,
                    state,
                    move |_d, s, args| {
                        let text = args
                            .first()
                            .map(|value| value.stringify().to_string())
                            .unwrap_or_default();
                        commit_field_value(s, &text, index, field_index);
                    },
                );
                let edit_for_blur = edit.clone();
                on_signal(
                    &edit.clone().upcast(),
                    "focus_exited",
                    &dialog_gd,
                    state,
                    move |_d, s, _args| {
                        commit_field_value(
                            s,
                            &edit_for_blur.get_text().to_string(),
                            index,
                            field_index,
                        );
                    },
                );
                add_any(&fields_box, &edit);
                let picker = picker_kind(&statement_name, field_index, &values);
                if !picker.is_empty() {
                    add_any(
                        &fields_box,
                        &picker_button(&dialog_gd, state, index, field_index, picker, color),
                    );
                }
                if !label_before && !label.is_empty() {
                    add_any(&fields_box, &field_label(&label));
                }
            }
            "select" => {
                if label_before && !label.is_empty() {
                    add_any(&fields_box, &field_label(&label));
                }
                // `EffectStatement.build` swaps the enum select for a 150-wide
                // wrapped button that opens `ui.effects`
                // (`LStatements.java:1801-1812`).
                if statement_name == "effect" && field_index == 0 {
                    add_any(
                        &fields_box,
                        &effect_button(&dialog_gd, state, index, field_index, color),
                    );
                    continue;
                }
                // Java's two-state `table.button`s, not `showSelect` popups:
                // apply/clear (`:1437-1444`), global/positional (`:2192-2196`),
                // circle/rect (`:1227-1231`).
                if statement_name == "status" && field_index == 0 {
                    add_any(
                        &fields_box,
                        &field_toggle(
                            &dialog_gd,
                            state,
                            index,
                            field_index,
                            &value,
                            ToggleSpec {
                                on_value: "true",
                                off_value: "false",
                                on_label: "clear",
                                off_label: "apply",
                                size: Vector2::new(80.0, 40.0),
                            },
                            color,
                        ),
                    );
                    continue;
                }
                if statement_name == "playsound" && field_index == 0 {
                    add_any(
                        &fields_box,
                        &field_toggle(
                            &dialog_gd,
                            state,
                            index,
                            field_index,
                            &value,
                            ToggleSpec {
                                on_value: "true",
                                off_value: "false",
                                on_label: "positional",
                                off_label: "global",
                                size: Vector2::new(160.0, 40.0),
                            },
                            color,
                        ),
                    );
                    continue;
                }
                if statement_name == "query" && field_index == 0 {
                    add_any(
                        &fields_box,
                        &field_toggle(
                            &dialog_gd,
                            state,
                            index,
                            field_index,
                            &value,
                            ToggleSpec {
                                on_value: "rect",
                                off_value: "circle",
                                on_label: "rect",
                                off_label: "circle",
                                size: Vector2::new(80.0, 40.0),
                            },
                            color,
                        ),
                    );
                    continue;
                }
                let mut button =
                    factory::button(&select_text(&statement_name, field_index, &value));
                // `Styles.logict` tinted with the element colour.
                button.set_theme_type_variation("logict");
                apply_logict(&mut button, color);
                let size = select_button_size(&statement_name, field_index, &value);
                button.set_custom_minimum_size(size);
                let statement_for_select = statement_name.clone();
                on_pressed(&button, &dialog_gd, state, move |d, s| {
                    open_select(d, s, index, field_index, &statement_for_select);
                });
                add_any(&fields_box, &button);
            }
            "color" => {
                // code-instantiated: one colour picker per colour field.
                let mut picker = ColorPickerButton::new_alloc();
                picker.set_custom_minimum_size(Vector2::new(40.0, 40.0));
                picker.set_pick_color(parse_color(&value));
                on_signal(
                    &picker.clone().upcast(),
                    "color_changed",
                    &dialog_gd,
                    state,
                    move |_d, s, args| {
                        let color = args
                            .first()
                            .and_then(|value| value.try_to::<Color>().ok())
                            .unwrap_or(Color::WHITE);
                        set_color(s, color, index, field_index);
                    },
                );
                add_any(&fields_box, &picker);
            }
            "align" => {
                let mut button = factory::image_button("pencil", "logict");
                apply_logict(&mut button, color);
                button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
                button.set_tooltip_text(&gs(field_label_text("align")));
                on_pressed(&button, &dialog_gd, state, move |d, s| {
                    open_align_select(d, s, index, field_index);
                });
                add_any(&fields_box, &button);
            }
            "char" => {
                let mut button = factory::image_button("pencil", "logict");
                apply_logict(&mut button, color);
                button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
                on_pressed(&button, &dialog_gd, state, move |d, s| {
                    open_char_select(d, s, index, field_index);
                });
                add_any(&fields_box, &button);
            }
            "jump" => {
                // `table.add().growX(); table.add(new JumpButton(...)).size(30).right()`.
                // code-instantiated: the growX spacer that pushes the node right.
                let mut push = Control::new_alloc();
                push.set_h_size_flags(SizeFlags::EXPAND_FILL);
                push.set_mouse_filter(MouseFilter::IGNORE);
                fields_box.add_child(&push);
                add_any(&fields_box, &jump_button(&dialog_gd, state, index));
            }
            _ => {}
        }
    }
    fields_box.upcast()
}

/// `LStatements.java:1483-1485` (`ApplyStatusStatement.isPermanent`): the
/// `seconds` field disappears for a `permanent` status.
pub(super) fn filter_layout(statement_name: &str, values: &[String], items: VarArray) -> VarArray {
    if statement_name != "status" || values.len() < 2 || values[0] == "true" {
        return items;
    }
    if !status_permanent(&values[1]) {
        return items;
    }
    let mut out = VarArray::new();
    for entry in items.iter_shared() {
        let keep = match entry.try_to::<VarDictionary>() {
            Ok(dict) => dict_string(&dict, "label") != "seconds",
            Err(_) => true,
        };
        if keep {
            out.push(&entry);
        }
    }
    out
}

/// `content.statusEffect(effect.substring(8)).permanent`, through the sim's
/// content snapshot (`MindSimHost.status_permanent`).
pub(super) fn status_permanent(effect: &str) -> bool {
    if !effect.starts_with("@status-") {
        return false;
    }
    let Some(mut host) = kit::sim_host() else {
        return false;
    };
    if !host.has_method("status_permanent") {
        return false;
    }
    host.call("status_permanent", &[gs(effect).to_variant()])
        .try_to::<bool>()
        .unwrap_or(false)
}

/// The content picker bound to a statement field, if any (`CONTENT_PICKERS`;
/// `LStatements.java` `showSelectTable` call sites).
pub(super) fn picker_kind(
    statement_name: &str,
    field_index: i64,
    values: &[String],
) -> &'static str {
    let first = values.first().map(String::as_str).unwrap_or("");
    if statement_name == "setmarker" && first != "drawLayer" {
        return "";
    }
    if statement_name == "ulocate" && first != "ore" {
        return "";
    }
    content_picker(statement_name, field_index)
}

/// The two-state `logict` button data.
pub(super) struct ToggleSpec {
    pub(super) on_value: &'static str,
    pub(super) off_value: &'static str,
    pub(super) on_label: &'static str,
    pub(super) off_label: &'static str,
    pub(super) size: Vector2,
}

/// A two-state `logict` button (`Table.button(label, style, ...)`): pressing
/// commits the other value and rebuilds the element (`build(table)`).
pub(super) fn field_toggle(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    value: &str,
    spec: ToggleSpec,
    color: Color,
) -> Gd<Button> {
    // code-instantiated: one toggle per two-state statement field.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("logict");
    apply_logict(&mut button, color);
    button.set_custom_minimum_size(spec.size);
    let label = if value == spec.on_value {
        spec.on_label
    } else {
        spec.off_label
    };
    button.set_text(&gs(field_label_text(label)));
    let next = if value == spec.on_value {
        spec.off_value
    } else {
        spec.on_value
    };
    on_pressed(&button, dialog, state, move |d, s| {
        commit_field_value(s, next, index, field_index);
        s.borrow_mut().modified = true;
        rebuild_list(d, s);
    });
    button
}

/// A `Icon.pencilSmall` content-picker button (`LStatements.java`
/// `t.button(b -> { b.image(Icon.pencilSmall); ... }, Styles.logict, ...)`).
pub(super) fn picker_button(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    kind: &str,
    color: Color,
) -> Gd<Button> {
    // code-instantiated: one picker pencil per statement field.
    let mut button = factory::icon_button("pencil", "", "logict", AlignmentMode::BEGIN);
    apply_logict(&mut button, color);
    button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
    let dialog = dialog.clone();
    let state = state.clone();
    let kind = kind.to_string();
    let callable = Callable::from_fn("logic_picker", move |_args: &[&Variant]| {
        if dialog.is_instance_valid() {
            let mut handle = dialog.clone();
            let mut bound = handle.bind_mut();
            open_picker(&mut bound, &state, index, field_index, &kind);
        }
        Variant::nil()
    });
    let mut node: Gd<Node> = button.clone().upcast();
    node.connect("pressed", &callable);
    button
}

/// Dispatches the content pencil to its `showSelectTable` equivalent
/// (`LStatements.java` `showSelectTable` call sites).
pub(super) fn open_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    kind: &str,
) {
    // Content pickers (`showSelectTable`): dispatched per `CONTENT_PICKERS`.
    match kind {
        "unit" => open_icon_grid(dialog, state, index, field_index, "unit", "unit-control"),
        "ore" => open_icon_grid(dialog, state, index, field_index, "item", "item-ore"),
        "status" => open_status_picker(dialog, state, index, field_index),
        "weather" => open_weather_picker(dialog, state, index, field_index),
        "layer" => open_layer_picker(dialog, state, index, field_index),
        "prop" => open_prop_picker(dialog, state, index, field_index),
        "sensor" => open_sensor_picker(dialog, state, index, field_index),
        "sound" => open_sound_picker(dialog, state, index, field_index),
        _ => {}
    }
}
