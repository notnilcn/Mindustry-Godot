// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Palette data: the filtered/sorted block rows and their icons.
//! Upstream: core/src/mindustry/editor/MapEditorDialog.java

use super::free::palette_row;
use super::*;

#[godot_api(secondary)]
impl MindEditor {
    // --- palette (behaviour: filter + upstream sort, plan 19 §3.10) ---

    /// Editor block palette rows in Java's order
    /// (`mind_core::editor::palette::palette_order`,
    /// `MapEditorDialog.java:846-864`), filtered by an ASCII `search` on the
    /// internal name (the shell searches `localizedName` itself and passes "").
    #[func]
    pub fn palette_blocks(&self, search: GString) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        let Some(content) = self.content.as_ref() else {
            return out;
        };
        let needle = search.to_string().to_ascii_lowercase();
        for def in mind_core::editor::palette::palette_order(content.blocks()) {
            if needle.is_empty() || def.name.to_ascii_lowercase().contains(needle.as_str()) {
                out.push(&palette_row(def));
            }
        }
        out
    }

    /// One block's editor metadata (`ui_icon`, config/size flags) by name.
    #[func]
    pub fn block_info(&self, name: GString) -> VarDictionary {
        let Some(content) = self.content.as_ref() else {
            return VarDictionary::new();
        };
        match content.block_by_name(&name.to_string()) {
            Some(def) => palette_row(def),
            None => VarDictionary::new(),
        }
    }

    /// Base teams for the toolbar (`[{id, name, color}]`).
    #[func]
    pub fn palette_teams(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        for team in Team::base_teams() {
            let mut row = VarDictionary::new();
            row.set(&GString::from("id"), &(team.id as i64).to_variant());
            row.set(
                &GString::from("name"),
                &GString::from(team.name.as_str()).to_variant(),
            );
            row.set(
                &GString::from("color"),
                &team.color.to_rgba8888().to_variant(),
            );
            out.push(&row);
        }
        out
    }

    /// Available brush radii.
    #[func]
    pub fn brush_sizes(&self) -> PackedInt64Array {
        let mut out = PackedInt64Array::new();
        for size in BRUSH_SIZES {
            out.push((size * 10.0) as i64);
        }
        out
    }
}
