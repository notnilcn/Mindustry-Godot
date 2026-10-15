// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Filter option descriptors (`maps/filters/FilterOption.java`, plan 06 §3.8).
//!
//! Upstream `FilterOption` builds Arc scene widgets; here it is pure data
//! (`mind-core` owns the option list, the Godot dialog renders it). The block
//! predicates keep the `!headless && atlas.isFound(icon) && inEditor` shape but
//! the atlas check is behind plan 03's `IconLookup`; headless core applies the
//! kind/`in_editor` checks only (plan 06 §3.8).
//!
//! Each descriptor carries the serialized `field` its value lives in (the
//! `genfilters` JSON key; `FilterOption`'s getter/setter pair upstream), so the
//! dialog can render values and write edits without a per-filter table.

use crate::content::{BlockId, ContentRegistry};

use super::block_info;

/// One configurable filter field (`FilterOption` subclasses as data).
#[derive(Debug, Clone, PartialEq)]
pub enum FilterOption {
    /// `SliderOption` (min/max/step; `display=false` hides the numeric field).
    Slider {
        /// Bundle key suffix (`filter.option.<name>`).
        name: &'static str,
        /// `genfilters` JSON field.
        field: &'static str,
        /// Minimum value.
        min: f32,
        /// Maximum value.
        max: f32,
        /// Step.
        step: f32,
        /// Whether the numeric readout is shown.
        display: bool,
        /// Java's setter casts to `int` (`angle`/`radius`/`amount`).
        integer: bool,
    },
    /// `BlockOption` (content picker with a validity predicate).
    Block {
        /// Bundle key suffix.
        name: &'static str,
        /// `genfilters` JSON field.
        field: &'static str,
        /// Allowed blocks.
        predicate: BlockPredicate,
    },
    /// `ToggleOption`.
    Toggle {
        /// Bundle key suffix.
        name: &'static str,
        /// `genfilters` JSON field.
        field: &'static str,
    },
    /// A custom editor action (`LogicFilter` code button).
    Custom {
        /// Bundle key suffix.
        name: &'static str,
        /// `genfilters` JSON field.
        field: &'static str,
        /// Opaque action key consumed by the Godot dialog.
        action_key: &'static str,
    },
}

impl FilterOption {
    /// Builds a float slider descriptor (Java `new SliderOption(name, ...)`).
    pub fn slider(name: &'static str, field: &'static str, min: f32, max: f32, step: f32) -> Self {
        FilterOption::Slider {
            name,
            field,
            min,
            max,
            step,
            display: true,
            integer: false,
        }
    }

    /// Builds a slider whose setter casts to `int` (Java `f -> x = (int)f`).
    pub fn int_slider(
        name: &'static str,
        field: &'static str,
        min: f32,
        max: f32,
        step: f32,
    ) -> Self {
        FilterOption::Slider {
            name,
            field,
            min,
            max,
            step,
            display: true,
            integer: true,
        }
    }

    /// Builds a block picker descriptor.
    pub fn block(name: &'static str, field: &'static str, predicate: BlockPredicate) -> Self {
        FilterOption::Block {
            name,
            field,
            predicate,
        }
    }

    /// Builds a toggle descriptor.
    pub fn toggle(name: &'static str, field: &'static str) -> Self {
        FilterOption::Toggle { name, field }
    }

    /// Builds a custom action descriptor.
    pub fn custom(name: &'static str, field: &'static str, action_key: &'static str) -> Self {
        FilterOption::Custom {
            name,
            field,
            action_key,
        }
    }

    /// The bundle key suffix.
    pub fn name(&self) -> &'static str {
        match self {
            FilterOption::Slider { name, .. }
            | FilterOption::Block { name, .. }
            | FilterOption::Toggle { name, .. }
            | FilterOption::Custom { name, .. } => name,
        }
    }

    /// The `genfilters` JSON field this option edits.
    pub fn field(&self) -> &'static str {
        match self {
            FilterOption::Slider { field, .. }
            | FilterOption::Block { field, .. }
            | FilterOption::Toggle { field, .. }
            | FilterOption::Custom { field, .. } => field,
        }
    }

    /// Serializes the descriptor for the Godot dialog.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            FilterOption::Slider {
                name,
                field,
                min,
                max,
                step,
                display,
                integer,
            } => serde_json::json!({
                "kind": "slider",
                "name": name,
                "field": field,
                "min": min,
                "max": max,
                "step": step,
                "display": display,
                "integer": integer,
            }),
            FilterOption::Block {
                name,
                field,
                predicate,
            } => serde_json::json!({
                "kind": "block",
                "name": name,
                "field": field,
                "predicate": predicate.name(),
            }),
            FilterOption::Toggle { name, field } => serde_json::json!({
                "kind": "toggle",
                "name": name,
                "field": field,
            }),
            FilterOption::Custom {
                name,
                field,
                action_key,
            } => serde_json::json!({
                "kind": "custom",
                "name": name,
                "field": field,
                "action": action_key,
            }),
        }
    }
}

/// Block validity predicates (`FilterOption.floorsOnly`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockPredicate {
    /// `FloorsOnly`: a floor that is not an overlay floor.
    FloorsOnly,
    /// `WallsOnly`: a non-synthetic, non-floor block.
    WallsOnly,
    /// `FloorsOptional`: air or a floor (not an overlay floor).
    FloorsOptional,
    /// `WallsOptional`: air or a wall.
    WallsOptional,
    /// `WallsOresOptional`: air, a wall, or an overlay floor.
    WallsOresOptional,
    /// `OresOnly`: an overlay floor.
    OresOnly,
    /// `OresFloorsOptional`: any floor (including overlays).
    OresFloorsOptional,
    /// `AnyOptional`: any floor/wall/ore, or air.
    AnyOptional,
}

/// The Java constant names (`FilterOption.java:23-30`), used as the wire name.
pub const BLOCK_PREDICATE_NAMES: [(&str, BlockPredicate); 8] = [
    ("floorsOnly", BlockPredicate::FloorsOnly),
    ("wallsOnly", BlockPredicate::WallsOnly),
    ("floorsOptional", BlockPredicate::FloorsOptional),
    ("wallsOptional", BlockPredicate::WallsOptional),
    ("wallsOresOptional", BlockPredicate::WallsOresOptional),
    ("oresOnly", BlockPredicate::OresOnly),
    ("oresFloorsOptional", BlockPredicate::OresFloorsOptional),
    ("anyOptional", BlockPredicate::AnyOptional),
];

impl BlockPredicate {
    /// The Java constant name.
    pub fn name(self) -> &'static str {
        BLOCK_PREDICATE_NAMES
            .iter()
            .find(|(_, predicate)| *predicate == self)
            .map(|(name, _)| *name)
            .unwrap_or("anyOptional")
    }

    /// Parses a Java constant name (`None` when unknown).
    pub fn from_name(name: &str) -> Option<Self> {
        BLOCK_PREDICATE_NAMES
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, predicate)| *predicate)
    }

    /// Evaluates the predicate (`Boolf<Block>`).
    pub fn test(self, registry: &ContentRegistry, block: BlockId) -> bool {
        let Some(def) = registry.block(block) else {
            return false;
        };
        let air = block == BlockId::AIR;
        let floors_only = block_info::floors_only(def);
        let walls_only = block_info::walls_only(def);
        let ores_only = block_info::is_overlay(def);
        match self {
            BlockPredicate::FloorsOnly => floors_only,
            BlockPredicate::FloorsOptional => air || floors_only,
            BlockPredicate::WallsOnly => walls_only,
            BlockPredicate::WallsOptional => air || (walls_only && def.in_editor),
            BlockPredicate::WallsOresOptional => {
                air || ((walls_only || ores_only) && def.in_editor)
            }
            BlockPredicate::OresOnly => ores_only,
            BlockPredicate::OresFloorsOptional => !air && block_info::is_floor(def),
            BlockPredicate::AnyOptional => {
                (floors_only || walls_only || ores_only || air) && (air || def.in_editor)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicates_round_trip_by_name() {
        for (name, predicate) in BLOCK_PREDICATE_NAMES {
            assert_eq!(BlockPredicate::from_name(name), Some(predicate));
            assert_eq!(predicate.name(), name);
        }
        assert_eq!(BlockPredicate::from_name("nope"), None);
    }

    #[test]
    fn option_serialization_carries_field() {
        let slider = FilterOption::slider("scale", "scl", 1.0, 500.0, 0.00001);
        assert_eq!(slider.field(), "scl");
        let json = slider.to_json();
        assert_eq!(json["kind"], "slider");
        assert_eq!(json["field"], "scl");
        let block = FilterOption::block("wall", "block", BlockPredicate::WallsOptional);
        assert_eq!(block.to_json()["predicate"], "wallsOptional");
        let custom = FilterOption::custom("code", "code", "logiccode");
        assert_eq!(custom.to_json()["kind"], "custom");
        let toggle = FilterOption::toggle("loop", "loop");
        assert_eq!(toggle.to_json()["kind"], "toggle");
    }
}
