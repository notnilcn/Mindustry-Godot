// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PathTile` bit packing (plan 11 §3.7, frozen layout).
//!
//! Ported from `core/src/mindustry/ai/Pathfinder.java` (`@Struct PathTileStruct`).
//! The 32-bit layout is a save/hash ABI; do not reorder bits.

use crate::content::{BlockId, ContentRegistry};
use crate::world::WorldGrid;

/// Health bits `0..8` (block hp / 40, capped at 80).
pub const HEALTH_MASK: u32 = 0xff;
/// Team bits `8..16` (`255` = derelict under core capture).
pub const TEAM_SHIFT: u32 = 8;
/// Solid block bit 16.
pub const BIT_SOLID: u32 = 1 << 16;
/// Liquid floor bit 17.
pub const BIT_LIQUID: u32 = 1 << 17;
/// Leg-solid bit 18.
pub const BIT_LEG_SOLID: u32 = 1 << 18;
/// Near-liquid bit 19.
pub const BIT_NEAR_LIQUID: u32 = 1 << 19;
/// Near-ground bit 20.
pub const BIT_NEAR_GROUND: u32 = 1 << 20;
/// Near-solid bit 21.
pub const BIT_NEAR_SOLID: u32 = 1 << 21;
/// Near-leg-solid bit 22.
pub const BIT_NEAR_LEG_SOLID: u32 = 1 << 22;
/// Deep-liquid bit 23.
pub const BIT_DEEP: u32 = 1 << 23;
/// Damaging-floor bit 24.
pub const BIT_DAMAGES: u32 = 1 << 24;
/// All-deep bit 25.
pub const BIT_ALL_DEEP: u32 = 1 << 25;
/// Near-deep bit 26.
pub const BIT_NEAR_DEEP: u32 = 1 << 26;
/// Team-passable (no enemy block) bit 27.
pub const BIT_TEAM_PASSABLE: u32 = 1 << 27;

/// Packed pathfinding tile (frozen `@Struct`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PathTile(pub u32);

impl PathTile {
    /// Health bits (`block.hp / 40`).
    pub const fn health(self) -> u8 {
        (self.0 & HEALTH_MASK) as u8
    }

    /// Team bits.
    pub const fn team(self) -> u8 {
        ((self.0 >> TEAM_SHIFT) & 0xff) as u8
    }

    /// Whether the tile holds a solid block.
    pub const fn solid(self) -> bool {
        self.0 & BIT_SOLID != 0
    }

    /// Whether the floor is deep liquid.
    pub const fn deep(self) -> bool {
        self.0 & BIT_DEEP != 0
    }

    /// Whether the tile is a damaging floor.
    pub const fn damages(self) -> bool {
        self.0 & BIT_DAMAGES != 0
    }

    /// Raw packed value.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Builds a tile from explicit parts (test/port helper).
    pub fn from_parts(health: u8, team: u8, solid: bool, deep: bool, damages: bool) -> Self {
        let mut bits = (health as u32) & HEALTH_MASK;
        bits |= ((team as u32) << TEAM_SHIFT) & (0xff << TEAM_SHIFT);
        bits |= if solid { BIT_SOLID } else { 0 };
        bits |= if deep { BIT_DEEP } else { 0 };
        bits |= if damages { BIT_DAMAGES } else { 0 };
        Self(bits)
    }
}

/// Builds the world's packed pathfinding tiles for one team.
///
/// M0 encodes solid/health/team/damage bits from the block layer; floor
/// attributes (liquid/deep/legSolid) are plan 06 floor data and land with M3.
pub fn build_tiles(grid: &WorldGrid, content: &ContentRegistry, team: u8) -> Vec<PathTile> {
    let mut tiles = Vec::with_capacity(grid.len());
    for (_pos, index) in grid.iter_row_major() {
        let tile = grid.tile_ref(index);
        let block = tile.block;
        let def = content.block(block);
        let solid = def.map(|def| def.solid).unwrap_or(false);
        let health = def
            .map(|def| ((def.health as f32 / 40.0).min(80.0)) as u8)
            .unwrap_or(0);
        // Damaging floors come from plan 06 floor data (M3); block damage is 0.
        tiles.push(PathTile::from_parts(health, team, solid, false, false));
    }
    tiles
}

/// Whether `block` is a solid obstacle for ground pathing.
pub fn is_solid(content: &ContentRegistry, block: BlockId) -> bool {
    content.block(block).map(|def| def.solid).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_layout_is_frozen() {
        let tile = PathTile::from_parts(80, 3, true, true, true);
        assert_eq!(tile.health(), 80);
        assert_eq!(tile.team(), 3);
        assert!(tile.solid());
        assert!(tile.deep());
        assert!(tile.damages());
        // Exact bits: health 80 (0x50), team 3 << 8, solid 1<<16, deep 1<<23,
        // damages 1<<24.
        let expected = 80 | (3 << 8) | (1 << 16) | (1 << 23) | (1 << 24);
        assert_eq!(tile.raw(), expected);
    }

    #[test]
    fn empty_tile_is_all_zero() {
        let tile = PathTile::default();
        assert!(!tile.solid());
        assert_eq!(tile.health(), 0);
        assert_eq!(tile.team(), 0);
    }
}
