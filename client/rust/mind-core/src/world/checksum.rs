// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ChecksumPart` for the world grid (plan 06 §3.2, risk R10).
//!
//! Stream: width/height, then per tile row-major `block`, `floor`, `overlay`,
//! `data`, `floor_data`, `overlay_data`, `extra_data` and the building presence
//! flag. This is the canonical world part; plan 05's `Sim::checksum` currently
//! builds an equivalent stream inline and R10 records the swap when the
//! world part replaces it (goldens re-recorded together).

use crate::determinism::{ChecksumPart, Hasher};

use super::WorldGrid;

impl ChecksumPart for WorldGrid {
    fn checksum_into(&self, hasher: &mut Hasher) {
        hasher.write_u32(self.tiles.width as u32);
        hasher.write_u32(self.tiles.height as u32);
        hasher.write_u32(self.tile_changes as u32);
        hasher.write_u32(self.floor_changes as u32);
        for tile in self.tiles.iter() {
            hasher.write_u16(tile.block.raw());
            hasher.write_u16(tile.floor.raw());
            hasher.write_u16(tile.overlay.raw());
            hasher.write_u8(tile.data as u8);
            hasher.write_u8(tile.floor_data as u8);
            hasher.write_u8(tile.overlay_data as u8);
            hasher.write_i32(tile.extra_data);
            hasher.write_bool(tile.build.is_some());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::determinism::Checksummer;

    #[test]
    fn checksum_changes_with_tile_content() {
        let mut a = WorldGrid::new(4, 4);
        let b = WorldGrid::new(4, 4);
        let mut ca = Checksummer::new();
        let mut cb = Checksummer::new();
        a.tiles.get_mut(1, 1).block = BlockId::STONE_WALL;
        ca.part(&a);
        cb.part(&b);
        assert_ne!(ca.finish(), cb.finish());
    }

    #[test]
    fn checksum_is_stable_for_identical_grids() {
        let a = WorldGrid::new(8, 8);
        let b = WorldGrid::new(8, 8);
        let mut ca = Checksummer::new();
        let mut cb = Checksummer::new();
        ca.part(&a);
        cb.part(&b);
        assert_eq!(ca.finish(), cb.finish());
    }
}
