// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `queries` support code.
//! Upstream: logic/LStatement.java, logic/LCanvas.java

use super::*;

// ---------------------------------------------------------------------------
// state queries (`LogicDialog` helpers)
// ---------------------------------------------------------------------------

/// `_target_width` / `LCanvas.getTargetWidth`: 410 when compact, else 95% of the
/// screen minus 80 clamped to 400..1200 (`isCompact`: width < 900 * 1.2).
pub(super) fn target_width(dialog: &MindDialog) -> f32 {
    let width = dialog.base().get_viewport_rect().size.x;
    if width < 900.0 * 1.2 {
        410.0
    } else {
        (width * 0.95 - 80.0).clamp(400.0, 1200.0)
    }
}

/// The current option's display text for a select (`LStatement.bundle(Enum)`
/// then the symbol/name fallback).
pub(super) fn select_text(statement_name: &str, field_index: i64, value: &str) -> String {
    let Some(mut logic) = kit::logic() else {
        return value.to_string();
    };
    if !logic.has_method("editor_options") {
        return value.to_string();
    }
    let options = logic.call(
        "editor_options",
        &[gs(statement_name).to_variant(), field_index.to_variant()],
    );
    let Ok(array) = options.try_to::<VarArray>() else {
        return value.to_string();
    };
    for option in array.iter_shared() {
        if let Ok(dict) = option.try_to::<VarDictionary>()
            && dict_string(&dict, "name") == value
        {
            return option_text(&dict);
        }
    }
    value.to_string()
}

pub(super) fn option_text(option: &VarDictionary) -> String {
    let mut text = {
        let label = dict_string(option, "label");
        if label.is_empty() {
            dict_string(option, "name")
        } else {
            label
        }
    };
    let key = dict_string(option, "key");
    // `LStatement.bundle(Enum)`/`selectTranslate` (`LStatement.java:163-184`):
    // no translation when `logiclocalization` is off.
    if !key.is_empty() && logic_localization() {
        let resolved = bundle(&key);
        if resolved != key {
            text = resolved;
        }
    }
    text
}

/// `LCanvas.tooltip(Cell, Enum)` (`LCanvas.java:98-105`): the
/// `<enum-class>.<name>` bundle key first, then `lenum.<name>`.
pub(super) fn select_tooltip(
    statement_name: &str,
    field_index: i64,
    option: &VarDictionary,
) -> String {
    let option_name = dict_string(option, "name");
    let mut enum_class = String::new();
    if statement_name == "op" {
        enum_class = "logicop".to_string();
    } else if (statement_name == "select" || statement_name == "jump") && field_index == 1 {
        enum_class = "conditionop".to_string();
    } else {
        let key = dict_string(option, "key");
        let suffix = format!(".label.{}", option_name.to_lowercase());
        if let Some(stripped) = key.strip_suffix(suffix.as_str()) {
            enum_class = stripped.to_string();
        }
    }
    if enum_class.is_empty() {
        return String::new();
    }
    let text = bundle_tooltip(&format!("{}.{}", enum_class, option_name.to_lowercase()));
    if text.is_empty() {
        bundle_tooltip(&format!("lenum.{option_name}"))
    } else {
        text
    }
}

/// `LStatement.showSelect`'s `cols` argument per call site.
pub(super) fn select_columns(statement_name: &str, field_index: i64) -> i32 {
    match statement_name {
        "draw" | "control" => {
            if field_index == 0 {
                2
            } else {
                4
            }
        }
        "radar" | "uradar" => {
            if field_index <= 3 {
                2
            } else {
                4
            }
        }
        "ucontrol" => {
            if field_index == 0 {
                2
            } else {
                4
            }
        }
        "ulocate" => {
            if field_index <= 1 {
                2
            } else {
                4
            }
        }
        "query" => 4,
        "select" | "jump" => {
            if field_index == 1 {
                3
            } else {
                4
            }
        }
        "setmarker" => {
            if field_index == 0 {
                3
            } else {
                4
            }
        }
        "makemarker" => {
            if field_index == 0 {
                2
            } else {
                4
            }
        }
        "setrule" | "message" | "fetch" => {
            if field_index == 0 {
                2
            } else {
                4
            }
        }
        "cutscene" if field_index == 0 => 3,
        _ => 4,
    }
}

/// The `sizer` cell override for a select (`showSelect(..., cols, sizer)`),
/// else the 60x38 default (`LStatement.showSelect`).
pub(super) fn select_cell_size(statement_name: &str, field_index: i64) -> Vector2 {
    match statement_name {
        "draw" => {
            if field_index == 0 {
                Vector2::new(120.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "control" => {
            if field_index == 0 {
                Vector2::new(110.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "radar" | "uradar" => {
            if field_index <= 2 {
                Vector2::new(110.0, 50.0)
            } else if field_index == 3 {
                Vector2::new(100.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "ucontrol" => {
            if field_index == 0 {
                Vector2::new(120.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "ulocate" => {
            if field_index == 0 {
                Vector2::new(110.0, 50.0)
            } else if field_index == 1 {
                Vector2::new(120.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "query" => {
            if field_index == 1 {
                Vector2::new(100.0, 38.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "select" | "jump" => {
            if field_index == 1 {
                Vector2::new(95.0, 38.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "setmarker" => {
            if field_index == 0 {
                Vector2::new(140.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "makemarker" => {
            if field_index == 0 {
                Vector2::new(160.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "setrule" | "message" | "fetch" => {
            if field_index == 0 {
                Vector2::new(150.0, 38.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "cutscene" => {
            if field_index == 0 {
                Vector2::new(120.0, 40.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "op" => {
            if field_index == 0 {
                Vector2::new(64.0, 38.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        _ => Vector2::new(60.0, 38.0),
    }
}

/// The select button's size (`table.button(...).size(...)` per call site).
pub(super) fn select_button_size(statement_name: &str, field_index: i64, value: &str) -> Vector2 {
    match statement_name {
        "draw" => {
            if field_index == 0 {
                Vector2::new(120.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "control" => {
            if field_index == 0 {
                Vector2::new(110.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "radar" | "uradar" => {
            if field_index <= 2 {
                Vector2::new(110.0, 40.0)
            } else if field_index == 3 {
                Vector2::new(180.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "ucontrol" => {
            if field_index == 0 {
                Vector2::new(180.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "ulocate" => {
            if field_index == 0 {
                Vector2::new(110.0, 40.0)
            } else if field_index == 1 {
                Vector2::new(120.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "query" => {
            if field_index == 1 {
                Vector2::new(100.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "select" | "jump" => {
            if field_index == 1 {
                if value == "always" {
                    Vector2::new(90.0, 40.0)
                } else {
                    Vector2::new(48.0, 40.0)
                }
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "setmarker" => {
            if field_index == 0 {
                Vector2::new(190.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "makemarker" => {
            if field_index == 0 {
                Vector2::new(180.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "setrule" | "message" | "fetch" => {
            if field_index == 0 {
                Vector2::new(160.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "cutscene" => {
            if field_index == 0 {
                Vector2::new(120.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "op" | "lookup" => {
            if field_index == 0 {
                Vector2::new(64.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "getblock" => {
            if field_index == 0 {
                Vector2::new(120.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "setblock" => {
            if field_index == 0 {
                Vector2::new(100.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        _ => Vector2::new(90.0, 40.0),
    }
}

pub(super) fn current_field_value(
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) -> String {
    if index < 0 || field_index < 0 {
        return String::new();
    }
    let st = state.borrow();
    let Some(row) = st.statements.get(index as usize) else {
        return String::new();
    };
    row.fields
        .get(field_index as usize)
        .cloned()
        .unwrap_or_default()
}

/// `LStatement.field`'s setter: `setter.get(sanitize(text))`. Returns whether
/// the value changed.
pub(super) fn commit_field_value(
    state: &Rc<RefCell<LogicState>>,
    text: &str,
    index: i64,
    field_index: i64,
) -> bool {
    if index < 0 || field_index < 0 {
        return false;
    }
    let Some(mut logic) = kit::logic() else {
        return false;
    };
    let sanitized = if logic.has_method("sanitize") {
        logic
            .call("sanitize", &[gs(text).to_variant()])
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_else(|_| text.to_string())
    } else {
        text.to_string()
    };
    let mut st = state.borrow_mut();
    let Some(row) = st.statements.get_mut(index as usize) else {
        return false;
    };
    let Some(slot) = row.fields.get_mut(field_index as usize) else {
        return false;
    };
    if *slot == sanitized {
        return false;
    }
    *slot = sanitized;
    st.modified = true;
    true
}

/// `JumpButton`'s setter: writes the `dest_index` field (`-1` clears).
pub(super) fn set_jump_target(state: &Rc<RefCell<LogicState>>, index: i64, target: i64) -> bool {
    if index < 0 {
        return false;
    }
    let mut st = state.borrow_mut();
    let Some(row) = st.statements.get_mut(index as usize) else {
        return false;
    };
    let Some(slot) = row.fields.first_mut() else {
        return false;
    };
    let next = target.to_string();
    if *slot == next {
        return false;
    }
    *slot = next;
    st.modified = true;
    true
}

/// Element references (`JumpStatement.dest`) survive an insertion at `at`:
/// every target at or below it moves down one (`saveUI` recomputes the index).
pub(super) fn shift_jumps_for_insert(state: &Rc<RefCell<LogicState>>, at: i64) {
    let mut st = state.borrow_mut();
    for row in st.statements.iter_mut() {
        if row.name != "jump" {
            continue;
        }
        let Some(slot) = row.fields.first_mut() else {
            continue;
        };
        let Ok(dest) = slot.parse::<i64>() else {
            continue;
        };
        if dest >= at {
            *slot = (dest + 1).to_string();
        }
    }
}

/// `StatementElem.copy` (`LCanvas.java:517-536`): the duplicate lands after the
/// element; a copied `jump`'s target bumps when it pointed below (`:521-526`).
pub(super) fn copy_statement(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>, index: i64) {
    let (count, original) = {
        let st = state.borrow();
        (
            st.statements.len(),
            st.statements.get(index.max(0) as usize).cloned(),
        )
    };
    let Some(original) = original else {
        return;
    };
    if index < 0 || count >= MAX_INSTRUCTIONS {
        return;
    }
    let mut copied = original.clone();
    shift_jumps_for_insert(state, index + 1);
    state
        .borrow_mut()
        .statements
        .insert(index as usize + 1, copied.clone());
    if original.name == "jump"
        && let Some(first) = copied.fields.first()
        && let Ok(dest) = first.parse::<i64>()
        && dest > index
    {
        copied.fields[0] = (dest + 1).to_string();
        state.borrow_mut().statements[index as usize + 1] = copied;
    }
    state.borrow_mut().modified = true;
    rebuild_list(dialog, state);
}

/// `StatementElem`'s delete button: dropped targets become `-1` and later
/// indices shift down (Java's `saveUI` recomputes `indexOf(dest)`).
pub(super) fn delete_statement(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
) {
    if index < 0 || state.borrow().statements.len() <= index as usize {
        return;
    }
    {
        let mut st = state.borrow_mut();
        st.statements.remove(index as usize);
        for row in st.statements.iter_mut() {
            if row.name != "jump" {
                continue;
            }
            let Some(slot) = row.fields.first_mut() else {
                continue;
            };
            let Ok(dest) = slot.parse::<i64>() else {
                continue;
            };
            if dest == index {
                *slot = "-1".to_string();
            } else if dest > index {
                *slot = (dest - 1).to_string();
            }
        }
        st.modified = true;
    }
    rebuild_list(dialog, state);
}

/// `shouldShowVariables`: an executor with variables outside the menu state.
/// The map editor and the filter/objective code editors run in `State.menu`
/// (`MapEditorDialog.java:317`), so they show `@logic.globals`; a live
/// processor always has `@counter`.
pub(super) fn should_show_variables(state: &Rc<RefCell<LogicState>>) -> bool {
    let (editor, filter, x, y) = {
        let st = state.borrow();
        (st.editor_mode, st.filter_mode, st.x, st.y)
    };
    if editor || filter {
        return false;
    }
    let Some(mut host) = kit::sim_host() else {
        return false;
    };
    if !host.has_method("logic_get_state") {
        return false;
    }
    let Ok(state_dict) = host
        .call("logic_get_state", &[x.to_variant(), y.to_variant()])
        .try_to::<VarDictionary>()
    else {
        return false;
    };
    let Some(vars) = state_dict.get("vars") else {
        return false;
    };
    vars.try_to::<VarArray>()
        .map(|array| !array.is_empty())
        .unwrap_or(false)
}

/// The variables button's label (`@variables` vs `@logic.globals`) and the
/// `@add` button's `disabled` (`>= maxInstructions`).
pub(super) fn update_buttons(state: &Rc<RefCell<LogicState>>) {
    let show_variables = should_show_variables(state);
    let label = if show_variables {
        bundle("@variables")
    } else {
        bundle("@logic.globals")
    };
    let variables = state.borrow().variables_button.clone();
    if let Some(button) = variables {
        set_button_text(&button, &label);
    }
    let full = state.borrow().statements.len() >= MAX_INSTRUCTIONS;
    let add = state.borrow().add_button.clone();
    if let Some(mut button) = add {
        button.set_disabled(full);
    }
}

/// Sets an `icon_button`'s label (the glyph row's label child, else `text`).
pub(super) fn set_button_text(button: &Gd<Button>, text: &str) {
    if !button.is_instance_valid() {
        return;
    }
    let children = button.get_children();
    if let Some(row) = children.iter_shared().next() {
        for (index, child) in row.get_children().iter_shared().enumerate() {
            if let Ok(label) = child.clone().try_cast::<Label>()
                && index > 0
                && !label.get_text().is_empty()
            {
                let mut label = label;
                label.set_text(&gs(text));
                return;
            }
        }
    }
    let mut button = button.clone();
    button.set_text(&gs(text));
}
