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

/// Floor metadata projection used by plan-11 movement/drowning consumers
/// (`Floor.{isDeep,isLiquid,shallow,speedMultiplier,drownTime}`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloorMeta {
    /// `Floor.isDeep()` (`drownTime > 0`).
    pub is_deep: bool,
    /// `Floor.isLiquid`.
    pub is_liquid: bool,
    /// `Floor.shallow`.
    pub shallow: bool,
    /// `Floor.speedMultiplier`.
    pub speed_multiplier: f32,
    /// `Floor.drownTime`.
    pub drown_time: f32,
}

/// Projects the floor fields for a floor block, or `None` for non-floors.
pub fn floor_meta(table: &BlockTable, block: crate::content::BlockId) -> Option<FloorMeta> {
    match &table.get(block)?.kind_data {
        crate::world::BlockKindData::Floor(def) => Some(FloorMeta {
            is_deep: def.is_deep,
            is_liquid: def.is_liquid,
            shallow: def.shallow,
            speed_multiplier: def.speed_multiplier,
            drown_time: def.drown_time,
        }),
        // `ShallowLiquid extends Floor`; `shallow` is always true.
        crate::world::BlockKindData::ShallowLiquid(def) => Some(FloorMeta {
            is_deep: def.drown_time > 0.0,
            is_liquid: def.is_liquid,
            shallow: def.shallow,
            speed_multiplier: def.speed_multiplier,
            drown_time: def.drown_time,
        }),
        _ => None,
    }
}

/// `Floor.isDeep` — a deep liquid allows drowning and blocks some placement
/// (`Floor.isDeep`, consumed by `build.rs`).
pub fn floor_is_deep(table: &BlockTable, block: crate::content::BlockId) -> bool {
    floor_meta(table, block).is_some_and(|meta| meta.is_deep)
}

/// `Floor.isLiquid` (plan-11 naval predicate).
pub fn floor_is_liquid(table: &BlockTable, block: crate::content::BlockId) -> bool {
    floor_meta(table, block).is_some_and(|meta| meta.is_liquid)
}

/// `Floor.speedMultiplier`, defaulting to `1.0` for non-floors/air.
pub fn floor_speed_multiplier(table: &BlockTable, block: crate::content::BlockId) -> f32 {
    floor_meta(table, block)
        .map(|meta| meta.speed_multiplier)
        .unwrap_or(1.0)
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

    #[test]
    fn floor_meta_reads_vanilla_fields() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");

        let deep = content.block_id("deep-water").expect("deep-water");
        let meta = floor_meta(&table, deep).expect("deep-water meta");
        assert!(meta.is_deep && meta.is_liquid && !meta.shallow);
        assert!((meta.speed_multiplier - 0.2).abs() < 1e-6);
        assert!((meta.drown_time - 200.0).abs() < 1e-6);

        // `ShallowLiquid` sets `isLiquid`/`shallow` and never drowns.
        let sand = content.block_id("sand-water").expect("sand-water");
        let meta = floor_meta(&table, sand).expect("sand-water meta");
        assert!(meta.is_liquid && meta.shallow && !meta.is_deep);
        assert!((meta.speed_multiplier - 0.8).abs() < 1e-6);

        let stone = content.block_id("stone").expect("stone");
        let meta = floor_meta(&table, stone).expect("stone meta");
        assert!(!meta.is_deep && !meta.is_liquid && !meta.shallow);
        assert_eq!(meta.speed_multiplier, 1.0);
        assert!(floor_speed_multiplier(&table, deep) < floor_speed_multiplier(&table, stone));
    }
}
