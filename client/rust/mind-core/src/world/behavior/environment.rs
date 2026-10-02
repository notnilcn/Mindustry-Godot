// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Environment block behavior (`world/blocks/environment/*`).
//!
//! Plan 07 §3.12 environment family: `Floor`/`OverlayFloor`/`OreBlock`/
//! `StaticWall`/`Prop`/`Cliff`/`ShallowLiquid`/`SteamVent`/`SpawnBlock`.
//!
//! None of these classes define an inner `Building` upstream (they are plain
//! `Block` subclasses), so `BlockInstance::has_building` is `false` for the
//! family and [`EnvironmentBehavior`] only exists to stop behavior dispatch from
//! falling into the generic path. The placement/derivation logic lives in plan
//! 06's tile ops and plan 07's `build.rs`; the draw data lives in `draw.rs`.
//! `SteamVent`'s 9-tile center indexing and `SpawnBlock`'s editor drawing are
//! recorded here as the hooks plan 06/19 call.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::world::block::BlockTable;

use super::BuildingBehavior;

/// Marker behavior for environment blocks (no building entity).
#[derive(Debug, Default, Clone, Copy)]
pub struct EnvironmentBehavior;

impl BuildingBehavior for EnvironmentBehavior {
    fn update_tile(&self, _world: &mut World, _e: Entity) {}
}

/// `SteamVent` occupies a 3×3 footprint but is indexed by its center tile only
/// (`SteamVent.update`/`shouldIndex`); the `SteamVent` [`BlockFlag`] marks it.
///
/// [`BlockFlag`]: crate::content::BlockFlag
pub fn is_steam_vent(table: &BlockTable, block: crate::content::BlockId) -> bool {
    table
        .view(block)
        .is_some_and(|view| view.has_flag(crate::content::BlockFlag::SteamVent))
}

/// `Floor.isDeep` — a deep liquid allows drowning and blocks some placement
/// (`Floor.isDeep`, consumed by `build.rs`).
pub fn floor_is_deep(table: &BlockTable, block: crate::content::BlockId) -> bool {
    table.get(block).is_some_and(|inst| match &inst.kind_data {
        crate::world::BlockKindData::Floor(def) => def.is_deep,
        _ => false,
    })
}

/// `OverlayFloor` may only be placed when `!wallOre || solid`
/// (`OverlayFloor.placeableOn`).
pub fn overlay_placeable_on_walls(wall_ore: bool, solid: bool) -> bool {
    !wall_ore || solid
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    #[test]
    fn environment_blocks_have_no_building() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        for name in ["stone", "sand", "moss", "spore-moss"] {
            if let Some(inst) = table.get_named(name) {
                assert_eq!(inst.kind_data.family_name(), "environment");
            }
        }
        let stone = content.block_id("stone").expect("stone");
        assert!(!floor_is_deep(&table, stone));
    }

    #[test]
    fn overlay_wall_rule() {
        assert!(overlay_placeable_on_walls(false, false));
        assert!(overlay_placeable_on_walls(true, true));
        assert!(!overlay_placeable_on_walls(true, false));
    }
}
