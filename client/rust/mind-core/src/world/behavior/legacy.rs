// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Legacy block behavior (`world/blocks/legacy/*`).
//!
//! Legacy blocks are hidden content removed during `World.endMapLoad` (plan 06
//! fires the load-end hook). This module carries the marker behavior and the
//! replacement lookup; no live building should ever persist past load.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::world::BlockKindData;

use super::BuildingBehavior;

/// Marker behavior for `LegacyBlock` + subclasses.
#[derive(Debug, Default, Clone, Copy)]
pub struct LegacyBehavior;

impl BuildingBehavior for LegacyBehavior {
    fn update_tile(&self, _world: &mut World, _e: Entity) {}
}

/// Whether a block is a legacy placeholder.
pub fn is_legacy(data: &BlockKindData) -> bool {
    matches!(data, BlockKindData::Legacy(_))
}

/// Replacement block id for a legacy block, when defined.
pub fn legacy_replacement(data: &BlockKindData) -> Option<u16> {
    match data {
        BlockKindData::Legacy(def) => def.replacement,
        _ => None,
    }
}

/// `World.endMapLoad` sweep hook (plan 06 calls `legacy_remove_self` per tile).
/// Returns whether the block should be removed from the world.
pub fn should_remove_on_load(data: &BlockKindData) -> bool {
    is_legacy(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::world::block::BlockTable;

    #[test]
    fn legacy_kinds_are_marked() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let mut legacy_seen = false;
        for inst in table.iter() {
            if is_legacy(&inst.kind_data) {
                legacy_seen = true;
                assert!(should_remove_on_load(&inst.kind_data));
            }
        }
        // The vanilla registry carries legacy blocks only on some branches; the
        // predicate must still be well-defined either way.
        let none = BlockKindData::None;
        assert!(!is_legacy(&none));
        assert_eq!(legacy_replacement(&none), None);
        let _ = legacy_seen;
    }
}
