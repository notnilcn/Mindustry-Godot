// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Filter option descriptors (`maps/filters/FilterOption.java`, plan 06 §3.8).
//!
//! Upstream `FilterOption` builds Arc scene widgets; here it is pure data (plan
//! 14 renders widgets, plan 19 supplies the editor flow). The block predicates
//! keep the `!headless && atlas.isFound(icon) && inEditor` shape but the atlas
//! check is behind plan 03's `IconLookup`; headless core applies the kind/
//! `in_editor` checks only (plan 06 §3.8).

use crate::content::{BlockId, ContentRegistry};

use super::block_info;

/// One configurable filter field (`FilterOption` subclasses as data).
#[derive(Debug, Clone, PartialEq)]
pub enum FilterOption {
    /// `SliderOption` (min/max/step; `display=false` hides the numeric field).
    Slider {
        /// Bundle key suffix (`filter.option.<name>`).
        name: &'static str,
        /// Minimum value.
        min: f32,
        /// Maximum value.
        max: f32,
        /// Step.
        step: f32,
        /// Whether the numeric readout is shown.
        display: bool,
    },
    /// `BlockOption` (content picker with a validity predicate).
    Block {
        /// Bundle key suffix.
        name: &'static str,
        /// Allowed blocks.
        predicate: BlockPredicate,
    },
    /// `ToggleOption`.
    Toggle {
        /// Bundle key suffix.
        name: &'static str,
    },
    /// A custom editor action (`LogicFilter` code button).
    Custom {
        /// Bundle key suffix.
        name: &'static str,
        /// Opaque action key consumed by plan 14/19.
        action_key: &'static str,
    },
}

impl FilterOption {
    /// Builds a slider descriptor.
    pub fn slider(name: &'static str, min: f32, max: f32, step: f32) -> Self {
        FilterOption::Slider {
            name,
            min,
            max,
            step,
            display: true,
        }
    }

    /// The bundle key suffix.
    pub fn name(&self) -> &'static str {
        match self {
            FilterOption::Slider { name, .. }
            | FilterOption::Block { name, .. }
            | FilterOption::Toggle { name }
            | FilterOption::Custom { name, .. } => name,
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

impl BlockPredicate {
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
            BlockPredicate::WallsOnly => walls_only,
            BlockPredicate::FloorsOptional => air || floors_only,
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
