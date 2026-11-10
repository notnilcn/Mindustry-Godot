// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Standalone-menu dialogs: about, credits, Discord and full text. Filled by S06 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod about;
pub mod credits;
pub mod discord;
pub mod full_text;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    about::dispatch(id)
        .or_else(|| credits::dispatch(id))
        .or_else(|| discord::dispatch(id))
        .or_else(|| full_text::dispatch(id))
}
