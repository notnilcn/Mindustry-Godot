// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Deferred-action and per-frame-tick helpers for the dialog controllers.
//! Upstream: none (port-only; the `call_deferred`/frame-tick seams of the old batch files)

use std::cell::RefCell;
use std::rc::Rc;

use godot::prelude::*;

use crate::ui::dialog::MindDialog;

/// A queued one-shot action (`Callable::call_deferred` payload).
type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// Queues `action` for the idle phase.
///
/// A `MindUi` prompt signal (`confirm_result`/`text_input_result`) is emitted
/// from inside a `MindUi` method, so the answering handler must not call back
/// into the facade synchronously: `Gd::bind_mut` panics on a re-entrant borrow
/// (`PAR-0066`).
pub(crate) fn defer_action(name: &'static str, action: impl FnOnce() + 'static) {
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

/// Runs `action` on the next frame (the deferred seams).
///
/// The queue is drained from a `SceneTree.process_frame` connection installed
/// by [`install_frame_tick`]; `MindDialog` owns no `_process` of its own.
// A boxed one-shot closure; naming the alias would not make the type simpler.
#[allow(clippy::type_complexity)]
#[derive(Clone, Default)]
pub(crate) struct Deferred(Rc<RefCell<Vec<Box<dyn FnMut()>>>>);

impl Deferred {
    pub(crate) fn push(&self, action: impl FnMut() + 'static) {
        self.0.borrow_mut().push(Box::new(action));
    }

    pub(crate) fn drain(&self) {
        let queued: Vec<Box<dyn FnMut()>> = self.0.borrow_mut().drain(..).collect();
        for mut action in queued {
            action();
        }
    }
}

/// Installs the per-frame hook once (deferred queue drain + `tick`).
///
/// `dialog` is `&mut MindDialog` rather than `&Gd<MindDialog>`: every caller
/// already runs inside `bind_mut`, and re-binding the same object panics.
pub(crate) fn install_frame_tick(
    dialog: &mut MindDialog,
    meta_prefix: &str,
    callable_name: &'static str,
    key: &str,
    deferred: Deferred,
    mut tick: impl FnMut() + 'static,
) {
    let mut tree = dialog.base().get_tree();
    let meta = format!("{meta_prefix}{key}");
    if dialog.base().has_meta(&meta) {
        return;
    }
    // The connection outlives a dialog freed while the tree is alive (editor
    // dialogs, teardown), so it must never touch the node again: the guard runs
    // before the queued actions and the tick, and drops the connection.
    let dialog_gd = dialog.to_gd();
    let tree_for = tree.clone();
    let callable_slot: Rc<RefCell<Option<Callable>>> = Rc::new(RefCell::new(None));
    let slot_for = callable_slot.clone();
    let callable = Callable::from_fn(callable_name, move |_args: &[&Variant]| {
        if !dialog_gd.is_instance_valid() {
            if let Some(entry) = slot_for.borrow_mut().take() {
                let mut tree = tree_for.clone();
                if tree.is_connected("process_frame", &entry) {
                    tree.disconnect("process_frame", &entry);
                }
            }
            return Variant::nil();
        }
        deferred.drain();
        tick();
        Variant::nil()
    });
    *callable_slot.borrow_mut() = Some(callable.clone());
    let _ = tree.connect("process_frame", &callable);
    dialog.base_mut().set_meta(&meta, &true.to_variant());
}
