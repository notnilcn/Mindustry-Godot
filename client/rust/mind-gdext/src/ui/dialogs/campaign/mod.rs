// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Campaign dialogs: research, loadouts, sector stats/select and the planet view. Filled by S03, S04 and S07 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod campaign_complete;
pub mod campaign_rules;
pub mod launch_loadout;
pub mod loadout;
pub mod planet;
pub mod research;
pub mod research_root_select;
pub mod sector_select;
pub mod sector_stats;
pub mod util;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    research::dispatch(id)
        .or_else(|| research_root_select::dispatch(id))
        .or_else(|| loadout::dispatch(id))
        .or_else(|| launch_loadout::dispatch(id))
        .or_else(|| sector_stats::dispatch(id))
        .or_else(|| sector_select::dispatch(id))
        .or_else(|| campaign_rules::dispatch(id))
        .or_else(|| campaign_complete::dispatch(id))
        .or_else(|| planet::dispatch(id))
}
