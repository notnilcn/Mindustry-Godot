// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Map-editor dialogs. Filled by S08 and S09 (docs/PLANS).
//! Upstream: none (group module; see the leaves)

pub mod banned_content;
pub mod editor_export;
pub mod editor_import;
pub mod editor_map;
pub mod editor_maps;
pub mod editor_menu;
pub mod map_assets;
pub mod map_generate;
pub mod map_info;
pub mod map_load;
pub mod map_locales;
pub mod map_objectives;
pub mod map_processors;
pub mod map_resize;
pub mod sector_generate;
pub mod util;
pub mod wave_info;

use crate::ui::dialog::DialogController;

/// Dispatches one of this group's dialog ids to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    editor_import::dispatch(id)
        .or_else(|| editor_export::dispatch(id))
        .or_else(|| editor_menu::dispatch(id))
        .or_else(|| editor_map::dispatch(id))
        .or_else(|| map_info::dispatch(id))
        .or_else(|| map_resize::dispatch(id))
        .or_else(|| map_generate::dispatch(id))
        .or_else(|| wave_info::dispatch(id))
        .or_else(|| map_objectives::dispatch(id))
        .or_else(|| map_locales::dispatch(id))
        .or_else(|| map_processors::dispatch(id))
        .or_else(|| map_load::dispatch(id))
        .or_else(|| sector_generate::dispatch(id))
        .or_else(|| banned_content::dispatch(id))
        .or_else(|| editor_maps::dispatch(id))
        .or_else(|| map_assets::dispatch(id))
}
