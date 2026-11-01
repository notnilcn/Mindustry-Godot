// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Per-dialog controllers for the shared [`super::dialog::MindDialog`] shell.
//!
//! Each module ports one `client/ui/dialogs/*.gd` (or
//! `client/scenes/editor/*.gd`) script into a [`super::dialog::DialogController`]
//! implementation. The dialog node resolves its registry key from the manifest
//! and dispatches here, so the 46 scenes that inherit `mind_dialog_base.tscn`
//! keep one native root class.

pub mod batch_a;
pub mod batch_b;
pub mod batch_c;
pub mod batch_campaign2;
pub mod batch_d;
pub mod batch_e;
pub mod batch_f;
pub mod batch_g;
pub mod batch_h;
pub mod batch_i;
pub mod batch_j;
pub mod misc;
pub mod sector;
pub mod simple;

use super::dialog::DialogController;

/// The controller for a dialog registry key (`about`, `settings`, …).
pub fn controller_for(id: &str) -> Option<Box<dyn DialogController>> {
    if let Some(controller) = simple::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = misc::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = sector::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = batch_a::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = batch_b::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = batch_c::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = batch_d::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = batch_e::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = batch_campaign2::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = batch_f::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = batch_g::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = batch_h::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = batch_i::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = batch_j::dispatch(id) {
        return Some(controller);
    }
    None
}
