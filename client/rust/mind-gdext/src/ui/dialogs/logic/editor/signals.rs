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

/// A queued one-shot action (`Callable.call_deferred` payload).
pub(super) type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// A `Callable` that queues `action` for the idle phase (`call_deferred`).
pub(super) fn defer_action(name: &'static str, action: impl FnOnce() + 'static) {
    let slot: DeferredAction = Rc::new(RefCell::new(Some(Box::new(action))));
    let callable = Callable::from_fn(name, move |_args: &[&Variant]| {
        let queued = slot.borrow_mut().take();
        if let Some(action) = queued {
            action();
        }
        Variant::nil()
    });
    callable.call_deferred(&[]);
}
