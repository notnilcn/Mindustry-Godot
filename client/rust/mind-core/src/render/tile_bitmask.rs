// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TileBitmask` (`core/src/mindustry/world/blocks/TileBitmask.java`).
//!
//! The 8-direction autotile table used by `TileableLogicDisplay` (and every
//! autotiled block family): an 8-bit neighbour mask indexes [`VALUES`], whose
//! entry is the sprite suffix (`<block>-<value>`, 47 regions per block).

/// `TileBitmask.values` (256 entries, verbatim).
pub const VALUES: [u8; 256] = [
    39, 36, 39, 36, 27, 16, 27, 24, 39, 36, 39, 36, 27, 16, 27, 24, //
    38, 37, 38, 37, 17, 41, 17, 43, 38, 37, 38, 37, 26, 21, 26, 25, //
    39, 36, 39, 36, 27, 16, 27, 24, 39, 36, 39, 36, 27, 16, 27, 24, //
    38, 37, 38, 37, 17, 41, 17, 43, 38, 37, 38, 37, 26, 21, 26, 25, //
    3, 4, 3, 4, 15, 40, 15, 20, 3, 4, 3, 4, 15, 40, 15, 20, //
    5, 28, 5, 28, 29, 10, 29, 23, 5, 28, 5, 28, 31, 11, 31, 32, //
    3, 4, 3, 4, 15, 40, 15, 20, 3, 4, 3, 4, 15, 40, 15, 20, //
    2, 30, 2, 30, 9, 46, 9, 22, 2, 30, 2, 30, 14, 44, 14, 6, //
    39, 36, 39, 36, 27, 16, 27, 24, 39, 36, 39, 36, 27, 16, 27, 24, //
    38, 37, 38, 37, 17, 41, 17, 43, 38, 37, 38, 37, 26, 21, 26, 25, //
    39, 36, 39, 36, 27, 16, 27, 24, 39, 36, 39, 36, 27, 16, 27, 24, //
    38, 37, 38, 37, 17, 41, 17, 43, 38, 37, 38, 37, 26, 21, 26, 25, //
    3, 0, 3, 0, 15, 42, 15, 12, 3, 0, 3, 0, 15, 42, 15, 12, //
    5, 8, 5, 8, 29, 35, 29, 33, 5, 8, 5, 8, 31, 34, 31, 7, //
    3, 0, 3, 0, 15, 42, 15, 12, 3, 0, 3, 0, 15, 42, 15, 12, //
    2, 1, 2, 1, 9, 45, 9, 19, 2, 1, 2, 1, 14, 18, 14, 13, //
];

/// `TileBitmask.values[bits]`: the sprite index for an 8-neighbour mask
/// (`bits == 255` means "fully surrounded" and draws no frame).
pub const fn value(bits: u8) -> u8 {
    VALUES[bits as usize]
}

/// Number of autotile regions per block (`TileBitmask.load`).
pub const REGION_COUNT: usize = 47;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmask_table_matches_java_edges() {
        // `TileBitmask.java`: row 0 starts `39, 36, 39, 36, ...`, the last row
        // ends `..., 14, 18, 14, 13`.
        assert_eq!(&VALUES[..4], &[39, 36, 39, 36]);
        assert_eq!(&VALUES[252..], &[14, 18, 14, 13]);
        assert_eq!(VALUES.len(), 256);
        assert!(VALUES.iter().all(|value| (*value as usize) < REGION_COUNT));
        // Mask 0 (no neighbours) picks the lone-tile sprite 39.
        assert_eq!(value(0), 39);
    }
}
