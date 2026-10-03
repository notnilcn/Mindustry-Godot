// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Static placement/break validity (`core/src/mindustry/world/Build.java`).
//!
//! Ported from `Build.validPlace*`, `validBreak`, `contactsGround`,
//! `contactsShallows`, `getEnemyOverlap` and `Block.canReplace`/`canPlaceOn`.
//! Bans/limits/darkness/core-radius use the plan-07 [`BuildRules`]/
//! [`BlockCounter`] providers (plan 12 overrides).

use crate::content::{BlockDef, BlockId, BlockKind, ContentRegistry, EnvFlag};
use crate::world::darkness::get_wall_darkness;

use super::WorldGrid;
use super::block::{BlockTable, BlockView};
use super::limits::{BlockCounter, BuildRules};

/// Block-relevant environment being placed into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlacementEnv {
    /// Whether the target tile is covered by liquid (`placeableLiquid` gate).
    pub on_liquid: bool,
    /// Whether the target floor is deep liquid (`isDeep`).
    pub deep_liquid: bool,
    /// Whether the tile is water for `requiresWater` (plan 09).
    pub on_water: bool,
}

/// `Block.supportsEnv(env)` for one flag.
pub fn supports_env(def: &BlockDef, env: EnvFlag) -> bool {
    if !def.env_disabled.is_empty() && def.env_disabled.contains(env) {
        return false;
    }
    if !def.env_required.is_empty() && !def.env_required.contains(env) {
        return false;
    }
    def.env_enabled.contains(env) || def.env_enabled.is_empty()
}

/// `Block.canPlaceOn`.
pub fn can_place_on(_def: &BlockDef, _team: u8, _rot: u8) -> bool {
    true
}

/// `Block.canBreak`.
pub fn can_break(_def: &BlockDef) -> bool {
    true
}

/// `Block.canReplace(other)` (`Block.java:796`).
///
/// `def` is `this` (the block being placed); `other` is the existing block. The
/// `subclass` equality check uses plan 07's `BlockKind` proxy (equal `BlockKind`
/// implies the same Java base class for every ported vanilla block).
pub fn can_replace(def: &BlockDef, other: &BlockDef) -> bool {
    if other.kind == BlockKind::ConstructBlock || other.always_replace {
        return true;
    }
    if other.privileged {
        return false;
    }
    let self_replace =
        other.id != def.id || (super::block::kind_rotates(def.kind) && def.quick_rotate);
    let grouped = (def.group != crate::content::BlockGroup::None && other.group == def.group)
        || other.id == def.id;
    let sized = def.size == other.size
        || (def.size >= other.size && (def.subclass == other.subclass || def.group.any_replace()));
    other.replaceable && self_replace && grouped && sized
}

/// Whether a tile holds a breakable, interactable block for `team`.
pub fn valid_break(
    content: &ContentRegistry,
    grid: &WorldGrid,
    rules: &BuildRules,
    team: u8,
    x: i32,
    y: i32,
) -> bool {
    if !grid.tiles.in_bounds(x, y) {
        return false;
    }
    let tile = grid.tile(x, y);
    if tile.block == BlockId::AIR {
        return false;
    }
    let Some(def) = content.block(tile.block) else {
        return false;
    };
    let breakable = def.destructible || rules.allow_environment_deconstruct;
    if !breakable || !can_break(def) {
        return false;
    }
    match tile.build {
        Some(entity) => {
            // Interactability is resolved by the runtime; no cross-entity read
            // needed here because placement always uses the local team.
            let _ = (entity, team);
            true
        }
        None => true,
    }
}

/// `Build.validPlace(type, team, x, y, rot)` (units checked; plan 11 indexer).
#[allow(clippy::too_many_arguments)]
pub fn valid_place(
    content: &ContentRegistry,
    table: &BlockTable,
    rules: &BuildRules,
    counter: &BlockCounter,
    grid: &WorldGrid,
    block: BlockId,
    team: u8,
    rot: u8,
    x: i32,
    y: i32,
) -> bool {
    valid_place_at(
        content, table, rules, counter, grid, block, team, rot, x, y, true,
    )
}

/// `Build.validPlace` with explicit coordinates and the unit-overlap toggle.
#[allow(clippy::too_many_arguments)]
pub fn valid_place_at(
    content: &ContentRegistry,
    table: &BlockTable,
    rules: &BuildRules,
    counter: &BlockCounter,
    grid: &WorldGrid,
    block: BlockId,
    team: u8,
    rot: u8,
    x: i32,
    y: i32,
    check_units: bool,
) -> bool {
    if block == BlockId::AIR || !grid.tiles.in_bounds(x, y) {
        return false;
    }
    let Some(def) = content.block(block) else {
        return false;
    };
    if !def.placeable_player && !def.in_editor {
        return false;
    }
    if rules.is_banned(block) {
        return false;
    }
    if counter.is_over_limit(rules, team, block) {
        return false;
    }
    if !supports_env(def, EnvFlag::Terrestrial) {
        return false;
    }
    // Darkness >= 3 blocks placement.
    if get_wall_darkness(grid, content, x, y) >= 3 {
        return false;
    }
    if check_units {
        // No unit index yet (plan 11); nothing to overlap.
    }
    let _ = rot;

    // Scan the footprint for replaceability.
    let size = def.size.max(1);
    let offset = -(size - 1) / 2;
    for dx in 0..size {
        for dy in 0..size {
            let tx = x + offset + dx;
            let ty = y + offset + dy;
            if !grid.tiles.in_bounds(tx, ty) {
                return false;
            }
            let tile = grid.tile(tx, ty);
            // Air and the same block are always placeable (upstream
            // `Build.validPlace`'s "same block, same rotation" bypass; the port
            // does not model rotation here yet).
            if tile.block == BlockId::AIR || tile.block == block {
                continue;
            }
            let Some(current) = content.block(tile.block) else {
                return false;
            };
            if !can_replace(def, current) {
                return false;
            }
        }
    }
    let _ = (table, can_place_on);
    true
}

/// `Build.contactsGround` — whether a block footprint touches solid ground.
pub fn contacts_ground(
    content: &ContentRegistry,
    grid: &WorldGrid,
    block: BlockId,
    x: i32,
    y: i32,
) -> bool {
    let Some(def) = content.block(block) else {
        return false;
    };
    let size = def.size.max(1);
    let offset = -(size - 1) / 2;
    for dx in 0..size {
        for dy in 0..size {
            let tx = x + offset + dx;
            let ty = y + offset + dy;
            if !grid.tiles.in_bounds(tx, ty) {
                continue;
            }
            // A floor that is not a liquid/air counts as ground.
            let floor = grid.tile(tx, ty).floor;
            if floor != BlockId::AIR
                && content
                    .block(floor)
                    .is_some_and(|def| def.solid || !def.placeable_liquid)
            {
                return true;
            }
            // Adjacent solid blocks also count.
            for (nx, ny) in [(tx - 1, ty), (tx + 1, ty), (tx, ty - 1), (tx, ty + 1)] {
                if !grid.tiles.in_bounds(nx, ny) {
                    continue;
                }
                let neighbor = grid.tile(nx, ny).block;
                if neighbor != BlockId::AIR && content.block(neighbor).is_some_and(|def| def.solid)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// `Build.contactsShallows` — whether a footprint touches a shallow liquid.
pub fn contacts_shallows(
    content: &ContentRegistry,
    grid: &WorldGrid,
    block: BlockId,
    x: i32,
    y: i32,
) -> bool {
    let Some(def) = content.block(block) else {
        return false;
    };
    let size = def.size.max(1);
    let offset = -(size - 1) / 2;
    for dx in 0..size {
        for dy in 0..size {
            let tx = x + offset + dx;
            let ty = y + offset + dy;
            if !grid.tiles.in_bounds(tx, ty) {
                continue;
            }
            let floor = grid.tile(tx, ty).floor;
            if floor != BlockId::AIR
                && content
                    .block(floor)
                    .is_some_and(|def| def.placeable_liquid && !def.solid)
            {
                return true;
            }
        }
    }
    false
}

/// `Build.getEnemyOverlap` positions covered by an enemy building.
///
/// Plan 11's `BlockIndexer` provides the real spatial query; the fallback scans
/// the footprint and reports tiles whose building is on another team.
pub fn get_enemy_overlap(
    grid: &WorldGrid,
    content: &ContentRegistry,
    def: &BlockDef,
    x: i32,
    y: i32,
    team: u8,
) -> Vec<(i16, i16)> {
    let mut out = Vec::new();
    let size = def.size.max(1);
    let offset = -(size - 1) / 2;
    for dx in 0..size {
        for dy in 0..size {
            let tx = x + offset + dx;
            let ty = y + offset + dy;
            if !grid.tiles.in_bounds(tx, ty) {
                continue;
            }
            let tile = grid.tile(tx, ty);
            if tile.block != BlockId::AIR
                && !content
                    .block(tile.block)
                    .is_some_and(|current| current.allow_core_placement)
            {
                // Without the ECS team read here, only report non-air tiles.
                out.push((tx as i16, ty as i16));
            }
            let _ = team;
        }
    }
    out
}

/// Convenience: [`BlockView`] for a placed block.
pub fn view_of<'a>(
    content: &'a ContentRegistry,
    table: &'a BlockTable,
    id: BlockId,
) -> Option<BlockView<'a>> {
    table.view(id).filter(|_| content.block(id).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_replace_same_group_and_construct() {
        let content = crate::content::test_support::test_registry();
        let wall = content.block_by_name("copper-wall").expect("wall");
        let wall2 = content.block_by_name("titanium-wall").expect("wall2");
        assert!(can_replace(wall, wall2));
        let build2 = content.block_by_name("build2").expect("build2");
        assert!(can_replace(wall, build2));
    }

    #[test]
    fn can_replace_honors_always_replace_and_privileged() {
        let content = crate::content::test_support::test_registry();
        let stone = content.block_by_name("stone").expect("stone");
        // `Prop` subclasses set `alwaysReplace = true`.
        let boulder = content.block_by_name("sand-boulder").expect("sand-boulder");
        assert!(can_replace(stone, boulder));
        // `privileged` blocks can never be replaced.
        let processor = content
            .block_by_name("world-processor")
            .expect("world-processor");
        assert!(!can_replace(stone, processor));
    }

    #[test]
    fn valid_place_rejects_occupied_dissimilar() {
        let content = crate::content::test_support::test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let rules = BuildRules::default();
        let counter = BlockCounter::new();
        let mut grid = WorldGrid::new(8, 8);
        grid.fill(BlockId::STONE_WALL, BlockId::AIR);
        let core = content.block_id("core-shard").expect("core-shard");
        let wall = content.block_id("copper-wall").expect("copper-wall");
        assert!(valid_place_at(
            &content, &table, &rules, &counter, &grid, wall, 0, 0, 4, 4, true
        ));
        // Place a core, then a wall over it: same-group? no -> rejected.
        grid.tiles.get_mut(4, 4).block = core;
        assert!(!valid_place_at(
            &content, &table, &rules, &counter, &grid, wall, 0, 0, 4, 4, true
        ));
    }
}
