// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `variables` support code.
//! Upstream: logic/LogicDialog.java

use super::*;

// ---------------------------------------------------------------------------
// variables panel (`LogicDialog` `@variables` / `GlobalVarsDialog`)
// ---------------------------------------------------------------------------

/// `LogicDialog`'s `@variables` button (`:162-243`): the variables panel when
/// `shouldShowVariables()`, else the globals panel; the button label follows.
pub(super) fn open_variables_panel(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    if !should_show_variables(state) {
        open_globals_panel(dialog, state);
        return;
    }
    let Some(mut host) = kit::sim_host() else {
        return;
    };
    if !host.has_method("logic_get_state") {
        return;
    }
    let (x, y) = {
        let st = state.borrow();
        (st.x, st.y)
    };
    let state_value = host.call("logic_get_state", &[x.to_variant(), y.to_variant()]);
    let mut vars = VarArray::new();
    if let Ok(dict) = state_value.try_to::<VarDictionary>()
        && let Some(list) = dict.get("vars")
        && let Ok(array) = list.try_to::<VarArray>()
    {
        vars = array;
    }
    open_vars_panel(dialog, state, &vars);
}

/// The per-variable rows (`LogicDialog.java:183-247`): name (accent), the value
/// (140 wide, refreshed every 15 frames) and the type.
pub(super) fn open_vars_panel(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    vars: &VarArray,
) {
    let dialog_gd = dialog.to_gd();
    // code-instantiated: the variables panel is transient and data-driven.
    let mut popup = PopupPanel::new_alloc();
    dialog.base_mut().add_child(&popup);
    let mut column = VBoxContainer::new_alloc();
    column.set_custom_minimum_size(Vector2::new(420.0, 320.0));
    popup.add_child(&column);
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    column.add_child(&scroll);
    let mut list = VBoxContainer::new_alloc();
    list.set_h_size_flags(SizeFlags::EXPAND_FILL);
    scroll.add_child(&list);
    // code-instantiated: one row per executor variable (count is data-driven).
    let mut rows: Vec<(String, Gd<Label>)> = Vec::new();
    for entry in vars.iter_shared() {
        let Ok(entry) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        let mut row = HBoxContainer::new_alloc();
        let mut name_label = Label::new_alloc();
        name_label.set_text(&gs(format!(" {} ", dict_string(&entry, "name"))));
        name_label.add_theme_color_override("font_color", styles::ACCENT);
        row.add_child(&name_label);
        let mut value_label = Label::new_alloc();
        value_label.set_text(&gs(format_var_value(&entry)));
        value_label.set_custom_minimum_size(Vector2::new(140.0, 0.0));
        value_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        row.add_child(&value_label);
        let mut type_label = Label::new_alloc();
        type_label.set_text(&gs(format!(" {} ", dict_string(&entry, "type"))));
        type_label.set_custom_minimum_size(Vector2::new(120.0, 0.0));
        row.add_child(&type_label);
        list.add_child(&row);
        rows.push((dict_string(&entry, "name"), value_label));
    }
    let mut buttons_row = HBoxContainer::new_alloc();
    column.add_child(&buttons_row);
    let globals = factory::icon_button(
        "list",
        &bundle("@logic.globals"),
        "cleart",
        AlignmentMode::BEGIN,
    );
    {
        let dialog_for_globals = dialog_gd.clone();
        let state_for_globals = state.clone();
        let popup_for_globals = popup.clone();
        let callable = Callable::from_fn("logic_globals", move |_args: &[&Variant]| {
            let mut popup = popup_for_globals.clone();
            popup.hide();
            if dialog_for_globals.is_instance_valid() {
                let mut handle = dialog_for_globals.clone();
                let mut bound = handle.bind_mut();
                open_globals_panel(&mut bound, &state_for_globals);
            }
            Variant::nil()
        });
        let mut node: Gd<Node> = globals.clone().upcast();
        node.connect("pressed", &callable);
    }
    buttons_row.add_child(&globals);
    {
        let mut st = state.borrow_mut();
        st.vars_popup = Some(popup.clone());
        st.vars_rows = rows;
        st.vars_timer = 0;
    }
    connect_popup_free(&popup);
    popup.popup_centered();
}

/// `LogicDialog`'s value text: objects print through the sim, numbers as
/// integers within 0.00001 (`LogicDialog.java:206-215`).
pub(super) fn format_var_value(entry: &VarDictionary) -> String {
    entry
        .get("value")
        .map(|value| value.stringify().to_string())
        .unwrap_or_default()
}

/// The globals panel (`GlobalVarsDialog`): `@name` + its `lglobal.<name>`
/// description.
pub(super) fn open_globals_panel(dialog: &mut MindDialog, _state: &Rc<RefCell<LogicState>>) {
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("globals") {
        return;
    }
    // code-instantiated: the globals panel is transient and data-driven.
    let mut popup = PopupPanel::new_alloc();
    dialog.base_mut().add_child(&popup);
    let mut column = VBoxContainer::new_alloc();
    column.set_custom_minimum_size(Vector2::new(520.0, 380.0));
    popup.add_child(&column);
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    column.add_child(&scroll);
    let mut list = VBoxContainer::new_alloc();
    list.set_h_size_flags(SizeFlags::EXPAND_FILL);
    scroll.add_child(&list);
    // code-instantiated: one row per built-in global (count is data-driven).
    let rows = logic.call("globals", &[]);
    if let Ok(rows) = rows.try_to::<VarArray>() {
        for entry in rows.iter_shared() {
            let Ok(entry) = entry.try_to::<VarDictionary>() else {
                continue;
            };
            let mut row = HBoxContainer::new_alloc();
            let mut name_label = Label::new_alloc();
            name_label.set_text(&gs(format!(" {} ", dict_string(&entry, "name"))));
            name_label.set_custom_minimum_size(Vector2::new(150.0, 0.0));
            row.add_child(&name_label);
            let mut desc_label = Label::new_alloc();
            desc_label.set_text(&gs(bundle(&dict_string(&entry, "description"))));
            desc_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
            desc_label.set_custom_minimum_size(Vector2::new(320.0, 0.0));
            row.add_child(&desc_label);
            list.add_child(&row);
        }
    }
    connect_popup_free(&popup);
    popup.popup_centered();
}

/// The 15-frame variables refresh while the panel is shown
/// (`LogicDialog.java:204-222`).
pub(super) fn refresh_variables(state: &Rc<RefCell<LogicState>>) {
    let (visible, x, y) = {
        let st = state.borrow();
        (
            st.vars_popup
                .as_ref()
                .map(|popup| popup.is_instance_valid() && popup.is_visible())
                .unwrap_or(false),
            st.x,
            st.y,
        )
    };
    if !visible {
        let mut st = state.borrow_mut();
        if st
            .vars_popup
            .as_ref()
            .is_none_or(|popup| !popup.is_instance_valid())
        {
            st.vars_popup = None;
            st.vars_rows.clear();
        }
        return;
    }
    {
        let mut st = state.borrow_mut();
        st.vars_timer += 1;
        if st.vars_timer < 15 {
            return;
        }
        st.vars_timer = 0;
    }
    let Some(mut host) = kit::sim_host() else {
        return;
    };
    if !host.has_method("logic_get_state") {
        return;
    }
    let Ok(state_dict) = host
        .call("logic_get_state", &[x.to_variant(), y.to_variant()])
        .try_to::<VarDictionary>()
    else {
        return;
    };
    let Some(vars) = state_dict.get("vars") else {
        return;
    };
    let Ok(vars) = vars.try_to::<VarArray>() else {
        return;
    };
    for entry in vars.iter_shared() {
        let Ok(entry) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        let var_name = dict_string(&entry, "name");
        let value = format_var_value(&entry);
        let rows = state.borrow().vars_rows.clone();
        for (row_name, mut label) in rows {
            if row_name == var_name && label.is_instance_valid() {
                label.set_text(&gs(value.as_str()));
            }
        }
    }
}
