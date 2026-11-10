// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Moved to dialogs/{campaign,maps,schematics,pickers}/. Deleted in W5 (C01).

use crate::ui::dialog::DialogController;

pub(crate) use super::campaign::util::{Deferred, install_frame_tick};
pub(crate) use super::maps::map_list::{
    MapListState, maplist_add_pane_button, maplist_build, maplist_load_settings, maplist_maps,
    maplist_rebuild, maplist_refresh_registry,
};

pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
