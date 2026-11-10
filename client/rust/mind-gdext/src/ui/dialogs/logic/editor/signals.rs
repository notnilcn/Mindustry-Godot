// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `signals` support code.
//! Upstream: logic/LogicDialog.java

use super::*;

// ---------------------------------------------------------------------------
// signal plumbing (`Gd<MindDialog>` + shared state callbacks)
// ---------------------------------------------------------------------------

/// Connects a signal whose handler needs the dialog and the shared state.
///
/// The handler only runs from engine-emitted signals (never re-entrant into a
/// bound `MindDialog`), so binding the dialog here is safe.
pub(super) fn on_signal<F>(
    node: &Gd<Node>,
    signal: &str,
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    action: F,
) where
    F: Fn(&mut MindDialog, &Rc<RefCell<LogicState>>, &[&Variant]) + 'static,
{
    let dialog = dialog.clone();
    let state = state.clone();
    let callable = Callable::from_fn("logic_action", move |args: &[&Variant]| {
        if !dialog.is_instance_valid() || dialog.is_queued_for_deletion() {
            return Variant::nil();
        }
        let mut handle = dialog.clone();
        let mut bound = handle.bind_mut();
        action(&mut bound, &state, args);
        Variant::nil()
    });
    let mut node = node.clone();
    node.connect(signal, &callable);
}

/// `on_signal` for a zero-argument `pressed` handler.
pub(super) fn on_pressed<F>(
    button: &Gd<Button>,
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    action: F,
) where
    F: Fn(&mut MindDialog, &Rc<RefCell<LogicState>>) + 'static,
{
    let node: Gd<Node> = button.clone().upcast();
    on_signal(&node, "pressed", dialog, state, move |d, s, _args| {
        action(d, s);
    });
}

// Adopted from F2: the local body was byte-identical to the shared helper.
pub(super) use crate::ui::dialogs::common::defer::defer_action;
