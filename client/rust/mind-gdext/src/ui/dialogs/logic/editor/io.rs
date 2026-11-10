// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `io` support code.
//! Upstream: logic/LogicDialog.java, logic/LCanvas.java

use super::*;

// ---------------------------------------------------------------------------
// load / save (`LogicDialog.show` / `LCanvas.save` / `LogicBuild`)
// ---------------------------------------------------------------------------

/// `LogicDialog.show`: loads the processor's program into the statement list.
pub(super) fn show_loaded(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let (x, y) = (dialog.context_int("x"), dialog.context_int("y"));
    let editor_mode = dialog
        .context_value("editor")
        .map(|value| value.try_to::<bool>().unwrap_or(false))
        .unwrap_or(false);
    let filter_mode = dialog
        .context_value("filter")
        .map(|value| value.try_to::<bool>().unwrap_or(false))
        .unwrap_or(false);
    {
        let mut st = state.borrow_mut();
        st.x = x;
        st.y = y;
        st.editor_mode = editor_mode;
        st.filter_mode = filter_mode;
        st.force_restart = false;
    }
    close_sub_dialog(state, false);
    on_resize(dialog, state);
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if filter_mode {
        let privileged = dialog
            .context_value("privileged")
            .map(|value| value.try_to::<bool>().unwrap_or(true))
            .unwrap_or(true);
        let code = dialog.context_string("code").to_string();
        {
            let mut st = state.borrow_mut();
            st.privileged = privileged;
            st.code = code;
        }
    } else if editor_mode {
        let Some(mut editor) = kit::editor() else {
            return;
        };
        if !editor.has_method("logic_get_state") {
            return;
        }
        let value = editor.call("logic_get_state", &[x.to_variant(), y.to_variant()]);
        let (privileged, code) = if let Ok(dict) = value.try_to::<VarDictionary>() {
            (
                dict_bool(&dict, "privileged", false),
                dict_string(&dict, "code"),
            )
        } else {
            (false, String::new())
        };
        {
            let mut st = state.borrow_mut();
            st.privileged = privileged;
            st.code = code;
        }
    } else {
        let Some(mut host) = kit::sim_host() else {
            return;
        };
        if !host.has_method("logic_get_state") || !host.has_method("logic_get_code") {
            return;
        }
        let value = host.call("logic_get_state", &[x.to_variant(), y.to_variant()]);
        let privileged = value
            .try_to::<VarDictionary>()
            .map(|dict| dict_bool(&dict, "privileged", false))
            .unwrap_or(false);
        let code = host
            .call("logic_get_code", &[x.to_variant(), y.to_variant()])
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default();
        {
            let mut st = state.borrow_mut();
            st.privileged = privileged;
            st.code = code;
        }
    }
    if !logic.has_method("parse_statements") {
        return;
    }
    let (code, privileged) = {
        let st = state.borrow();
        (st.code.clone(), st.privileged)
    };
    let parsed = logic.call(
        "parse_statements",
        &[gs(code.as_str()).to_variant(), privileged.to_variant()],
    );
    let mut rows = rows_from_variant(parsed);
    // `LCanvas.load`: `statements.truncate(LExecutor.maxInstructions)`.
    rows.truncate(MAX_INSTRUCTIONS);
    {
        let mut st = state.borrow_mut();
        st.statements = rows;
        st.modified = false;
        st.curve_heights.clear();
    }
    if let Some(mut scroll) = state.borrow().scroll.clone() {
        scroll.set_v_scroll(0);
    }
    rebuild_list(dialog, state);
    update_buttons(state);
}

/// `LCanvas.save` -> `LogicBuild.showEditDialog`'s consumer.
pub(super) fn save(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("write_statements") {
        return;
    }
    let rows = state.borrow().statements.clone();
    let code = logic
        .call("write_statements", &[rows_to_variant(&rows).to_variant()])
        .try_to::<GString>()
        .map(|value| value.to_string())
        .unwrap_or_default();
    state.borrow_mut().force_restart = false;
    let (filter_mode, editor_mode, x, y) = {
        let st = state.borrow();
        (st.filter_mode, st.editor_mode, st.x, st.y)
    };
    if filter_mode {
        // `modified.get(result)`: the generate dialog owns the filter JSON.
        {
            let mut st = state.borrow_mut();
            st.code = code.clone();
            st.modified = false;
        }
        let mut node = dialog.to_gd();
        node.emit_signal("code_edited", &[gs(code.as_str()).to_variant()]);
        return;
    }
    let error = if editor_mode {
        let Some(mut editor) = kit::editor() else {
            return;
        };
        if !editor.has_method("logic_configure_code") {
            return;
        }
        editor
            .call(
                "logic_configure_code",
                &[
                    x.to_variant(),
                    y.to_variant(),
                    gs(code.as_str()).to_variant(),
                ],
            )
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default()
    } else {
        let Some(mut host) = kit::sim_host() else {
            return;
        };
        if !host.has_method("logic_configure_code") {
            return;
        }
        host.call(
            "logic_configure_code",
            &[
                x.to_variant(),
                y.to_variant(),
                gs(code.as_str()).to_variant(),
            ],
        )
        .try_to::<GString>()
        .map(|value| value.to_string())
        .unwrap_or_default()
    };
    if error.is_empty() {
        let mut st = state.borrow_mut();
        st.code = code;
        st.modified = false;
    } else if let Some(length) = error.strip_prefix("toolong:") {
        show_toolong(length.trim().parse::<i64>().unwrap_or_default());
    }
}

pub(super) fn show_toolong(length: i64) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("toast") {
        return;
    }
    let mut text = "too long".to_string();
    if let Some(assets) = kit::assets() {
        let mut args = PackedStringArray::new();
        args.push(&gs("16000"));
        args.push(&gs(length.to_string().as_str()));
        text = assets
            .bind()
            .bundle_format(gs("logic.error.toolong"), args)
            .to_string();
    }
    let _ = ui.call(
        "toast",
        &[gs(text.as_str()).to_variant(), gs("error").to_variant()],
    );
}
