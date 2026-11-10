// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `edit_menu` support code.
//! Upstream: logic/LogicDialog.java

use super::*;

// ---------------------------------------------------------------------------
// edit menu (`LogicDialog`'s `@edit` / `@editor.export`)
// ---------------------------------------------------------------------------

/// `_open_edit_menu`: `BaseDialog("@editor.export")` with a `Tex.button` table
/// of `flatt` 280x60 rows (`@clear`, `@copy.clipboard`, `@load.clipboard`,
/// `@logic.restart`) and `addCloseButton`.
pub(super) fn open_edit_menu(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let dialog_gd = dialog.to_gd();
    let parts = open_sub_dialog(dialog, state, "@editor.export");
    let cont: Gd<Container> = parts.cont.clone().upcast();
    // code-instantiated: the `Tex.button` table of the edit menu (one per open).
    let mut margin = MarginContainer::new_alloc();
    for side in ["margin_left", "margin_right", "margin_top", "margin_bottom"] {
        margin.add_theme_constant_override(side, 10);
    }
    add_any(&cont, &margin);
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &button_box(state));
    margin.add_child(&panel);
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 0);
    panel.add_child(&column);
    column.add_child(&edit_row(&dialog_gd, state, "cancel", "@clear", |d, s| {
        edit_clear(d, s);
    }));
    column.add_child(&edit_row(
        &dialog_gd,
        state,
        "copy",
        "@copy.clipboard",
        |d, s| {
            edit_copy(d, s);
        },
    ));
    let mut load_row = edit_row(&dialog_gd, state, "download", "@load.clipboard", |d, s| {
        edit_load(d, s);
    });
    load_row.set_disabled(DisplayServer::singleton().clipboard_get().is_empty());
    column.add_child(&load_row);
    column.add_child(&edit_row(
        &dialog_gd,
        state,
        "refresh",
        "@logic.restart",
        |d, s| {
            edit_restart(d, s);
        },
    ));
}

/// One `t.button(text, icon, Styles.flatt, ...).size(280f, 60f).left()
/// .marginLeft(12f)`.
pub(super) fn edit_row<F>(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    icon: &str,
    key: &str,
    action: F,
) -> Gd<Button>
where
    F: Fn(&mut MindDialog, &Rc<RefCell<LogicState>>) + 'static,
{
    // code-instantiated: the edit menu rows are built per open.
    let mut button = factory::icon_button(icon, &bundle(key), "flatt", AlignmentMode::BEGIN);
    button.set_custom_minimum_size(Vector2::new(280.0, 60.0));
    button.add_theme_stylebox_override("normal", &flat_box_margins(Color::BLACK, 12.0));
    button.add_theme_stylebox_override("hover", &flat_box_margins(styles::FLAT_OVER, 12.0));
    button.add_theme_stylebox_override("pressed", &flat_box_margins(styles::FLAT_OVER, 12.0));
    on_pressed(&button, dialog, state, action);
    button
}

/// `@clear`: `ui.showConfirm("@logic.clear.confirm", canvas::clearStatements)`
/// then `dialog.hide()`.
pub(super) fn edit_clear(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    close_sub_dialog(state, true);
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("show_confirm") {
        return;
    }
    let needs_confirm_binding = !state.borrow().confirm_connected;
    if ui.has_signal("confirm_result") && needs_confirm_binding {
        let dialog_gd = dialog.to_gd();
        let state_for_confirm = state.clone();
        let callable = Callable::from_fn("logic_confirm", move |args: &[&Variant]| {
            let confirmed = args
                .first()
                .map(|value| value.try_to::<bool>().unwrap_or(false))
                .unwrap_or(false);
            if !state_for_confirm.borrow().pending_clear {
                return Variant::nil();
            }
            state_for_confirm.borrow_mut().pending_clear = false;
            if !confirmed {
                return Variant::nil();
            }
            // `LCanvas.clearStatements`.
            state_for_confirm.borrow_mut().statements.clear();
            state_for_confirm.borrow_mut().modified = true;
            if dialog_gd.is_instance_valid() {
                let mut handle = dialog_gd.clone();
                let mut bound = handle.bind_mut();
                rebuild_list(&mut bound, &state_for_confirm);
            }
            Variant::nil()
        });
        ui.connect("confirm_result", &callable);
        state.borrow_mut().confirm_connected = true;
    }
    state.borrow_mut().pending_clear = true;
    let _ = ui.call(
        "show_confirm",
        &[gs(bundle("@logic.clear.confirm")).to_variant()],
    );
}

/// `@copy.clipboard`: `Core.app.setClipboardText(canvas.save())`.
pub(super) fn edit_copy(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let _ = dialog;
    close_sub_dialog(state, true);
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("write_statements") {
        return;
    }
    let rows = state.borrow().statements.clone();
    let write = rows_to_variant(&rows);
    let code = logic
        .call("write_statements", &[write.to_variant()])
        .try_to::<GString>()
        .unwrap_or_default();
    DisplayServer::singleton().clipboard_set(&code);
}

/// `@load.clipboard`: `canvas.load(Core.app.getClipboardText())`.
pub(super) fn edit_load(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    close_sub_dialog(state, true);
    load_from_clipboard(dialog, state);
}

/// `@logic.restart`: `forceRestart = true; dialog.hide(); hide();` — the editor
/// closes and the consumer writes the program back even if unchanged.
pub(super) fn edit_restart(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    state.borrow_mut().force_restart = true;
    close_sub_dialog(state, true);
    dialog.close();
}

/// `@load.clipboard`: parse the clipboard text into the statement list.
pub(super) fn load_from_clipboard(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("parse_statements") {
        return;
    }
    let text = DisplayServer::singleton().clipboard_get();
    if text.to_string().trim().is_empty() {
        return;
    }
    let privileged = state.borrow().privileged;
    let parsed = logic.call(
        "parse_statements",
        &[text.to_variant(), privileged.to_variant()],
    );
    if let Ok(array) = parsed.try_to::<VarArray>() {
        let mut rows = rows_from_variant(array.to_variant());
        rows.truncate(MAX_INSTRUCTIONS);
        {
            let mut st = state.borrow_mut();
            st.statements = rows;
            st.modified = true;
        }
        rebuild_list(dialog, state);
    }
}
