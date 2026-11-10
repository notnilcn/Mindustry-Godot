// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Moved to `net/` (`util_misc`, `roster`, `bans`, `admins`, `trace`),
//! `pickers/list_select` and `campaign/campaign_complete`. Deleted in W5 (C01).

pub(crate) use super::net::trace::trace_context_json;

use crate::ui::dialog::DialogController;

/// The dialog controllers in this batch (all moved out).
pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
