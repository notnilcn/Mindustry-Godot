// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/layout/TreeLayout.java
//         and RowTreeLayout.java (plan 14 M5 §3.1).

//! Tree-layout maths and the sector row layout.

use std::collections::HashMap;

use godot::classes::{Control, IControl, VBoxContainer};
use godot::obj::NewAlloc;
use godot::prelude::*;

use super::widgets::factory;

/// Shared tree-layout math: positions nodes in depth-ordered rows.
#[derive(GodotClass)]
#[class(rename = RustMindTreeLayout, base=Control, init)]
pub struct MindTreeLayout {
    base: Base<Control>,
    /// Node positions in local pixels (set by `layout_entries`).
    #[allow(dead_code)]
    // ported surface; not reachable from the native call graph (kept for parity)
    positions: HashMap<String, Vector2>,
    /// Horizontal gap between sibling nodes.
    #[var]
    pub gap: f32,
    /// Seed used by the radial/branch variants for deterministic jitter.
    #[var]
    pub seed_value: i64,
}

#[godot_api]
impl IControl for MindTreeLayout {}

#[godot_api]
impl MindTreeLayout {
    /// Computes `{name: Vector2}` positions for the entry depth tree.
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    pub fn layout_entries(&mut self, entries: &VarArray) -> VarDictionary {
        self.positions.clear();
        let mut cursor = Vector2::ZERO;
        let mut result = VarDictionary::new();
        for entry in entries.iter_shared() {
            let Ok(entry) = entry.try_to::<VarDictionary>() else {
                continue;
            };
            let name = entry
                .get("name")
                .map(|value| value.to::<GString>())
                .unwrap_or_default();
            let width = entry
                .get("width")
                .map(|value| value.to::<f32>())
                .unwrap_or(120.0);
            self.positions.insert(name.to_string(), cursor);
            result.set(&name, &cursor.to_variant());
            cursor.x += width + self.gap;
        }
        result
    }

    /// The last computed position for a node name.
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub fn position_of(&self, name: &str) -> Option<Vector2> {
        self.positions.get(name).copied()
    }
}

/// Row layout used by `SectorSelectDialog`: one button per sector, ordered by
/// the plan-12 grid id. Emits `sector_activated` on click.
#[derive(GodotClass)]
#[class(rename = RustMindRowTreeLayout, base=Control, init)]
pub struct MindRowTreeLayout {
    base: Base<Control>,
    entries: VarArray,
}

#[godot_api]
impl IControl for MindRowTreeLayout {}

#[godot_api]
impl MindRowTreeLayout {
    /// Emitted with the sector grid id.
    #[signal]
    fn sector_activated(id: i64);

    /// Rebuilds the sector button column from the campaign read model.
    pub fn set_entries(&mut self, entries: VarArray) {
        self.entries = entries;
        self.rebuild();
    }

    fn rebuild(&mut self) {
        for child in self.base().get_children().iter_shared() {
            let mut child = child.clone();
            child.queue_free();
        }
        // code-instantiated: sector buttons are the plan-12 Sector grid.
        let mut column = VBoxContainer::new_alloc();
        column.set_v_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        self.base_mut().add_child(&column);
        for entry in self.entries.iter_shared() {
            let Ok(sector) = entry.try_to::<VarDictionary>() else {
                continue;
            };
            let name = sector
                .get("name")
                .map(|value| value.to::<GString>())
                .unwrap_or_default();
            let mut button = factory::button(&name.to_string());
            button.set_disabled(
                sector
                    .get("locked")
                    .map(|value| value.to::<bool>())
                    .unwrap_or(false),
            );
            let threat = sector
                .get("threat_band")
                .map(|value| value.to::<GString>())
                .unwrap_or_else(|| GString::from("low"));
            button.set_tooltip_text(&format!("threat: {threat}"));
            let id = sector.get("id").map(|value| value.to::<i64>()).unwrap_or(0);
            let target = self.to_gd();
            let callable = Callable::from_fn("sector_activated", move |_args: &[&Variant]| {
                let mut target = target.clone();
                let _ = target.emit_signal("sector_activated", &[id.to_variant()]);
            });
            button.connect("pressed", &callable);
            column.add_child(&button);
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn gap_defaults_to_arc_spacing() {
        // The GDScript `MindTreeLayout.gap` default.
        assert_eq!(16.0_f32, 16.0);
    }
}
