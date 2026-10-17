// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Floor.drawMain`'s pure pickers (`world/blocks/environment/Floor.java:264-292`):
//! the 8-neighbour autotile bitmask, the autotile/tiling region names and the
//! tiling cell math. The Godot half lives in `mind-gdext::render::floor`.
//!
//! Region naming (`Floor.load`, `:157-168`): autotile slices are `<name>-<i>`
//! (`i` = `TileBitmask.values[bits]`, 0..46), variant slices
//! `<name>-<v+1>-<i>` (`TileBitmask.loadVariants`), the mid slice for bit 13 is
//! `<name>-13` (pick 0) or `<name>-mid-<i+1>`; tiling variants are
//! `<name>-tile<v+1>`, split into cells of `tilesize / Draw.scl` pixels
//! (`Floor.load:131-137`; `Control.java:330` sets `Draw.scl` from the 4-px
//! `scale_marker`, so one cell is 32 sprite pixels = `tilesize` world units at
//! the 4x sprite supersample).

use crate::render::tile_bitmask;

/// `Geometry.d8` (`Geometry.java:25-34`): the neighbour order that indexes the
/// autotile bitmask — bit `i` is set when `d8[i]` passes `checkAutotileSame`.
pub const D8: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// `tilesize / Draw.scl` in sprite pixels (`Floor.load`: `(int)(tilesize /
/// Draw.scl)`): 8 world units / (1/4) = 32 px (`Control.java:330`, the 4-px
/// `scale_marker`).
pub const TILING_CELL_PX: i32 = 32;

/// The `Floor.drawMain` autotile bitmask: bit `i` (`D8[i]`) set when the
/// neighbour passes `checkAutotileSame`.
pub fn autotile_bits(same: [bool; 8]) -> u8 {
    let mut bits = 0u8;
    for (index, matches) in same.iter().enumerate() {
        if *matches {
            bits |= 1 << index;
        }
    }
    bits
}

/// `TileBitmask.values[bits]` (`TileBitmask.java:12-24`).
pub fn autotile_index(bits: u8) -> u8 {
    tile_bitmask::value(bits)
}

/// The autotile slice region for one tile (`Floor.drawMain:270-283`):
/// `regions = autotileVariants > 1 ? variantRow[variant] : autotileRegions`,
/// with the `bit == 13 && autotileMidVariants > 1` mid slice replacing it.
pub fn autotile_region(
    name: &str,
    bit: u8,
    autotile_variants: i32,
    variant_pick: u32,
    autotile_mid_variants: i32,
    mid_pick: u32,
) -> String {
    if bit == 13 && autotile_mid_variants > 1 {
        return if mid_pick == 0 {
            format!("{name}-13")
        } else {
            format!("{name}-mid-{}", mid_pick + 1)
        };
    }
    if autotile_variants > 1 {
        format!("{name}-{}-{bit}", variant_pick + 1)
    } else {
        format!("{name}-{bit}")
    }
}

/// The tiling split counted from the sprite size: `tile.split(tsize, tsize)`
/// (`Floor.load:135`) yields `sprite_px / TILING_CELL_PX` cells per axis.
/// `tilingSize = tilingRegions[i].length` (`:136`) is the **column** count and
/// is used for both axes by `drawMain:266-268`; `Floor.load:139-146` disables
/// tiling when the two axes disagree.
pub fn tiling_split(sprite_w: i32, sprite_h: i32) -> (u8, u8) {
    let cols = (sprite_w / TILING_CELL_PX).clamp(1, u8::MAX as i32) as u8;
    let rows = (sprite_h / TILING_CELL_PX).clamp(1, u8::MAX as i32) as u8;
    (cols, rows)
}

/// `Point2.pack(tile.x / tilingSize, tile.y / tilingSize)`
/// (`Floor.drawMain:266`): the block of tiles that picks the tiling variant
/// (`Mathf.randomSeed(pack, 0, tilingVariants - 1)`). Java integer division
/// truncates toward zero; negative coords cannot occur in a loaded map.
pub fn tiling_block(x: i32, y: i32, size: u8) -> (i32, i32) {
    let size = size.max(1) as i32;
    (x / size, y / size)
}

/// `tilingRegions[index][tile.x % tilingSize][tilingSize - 1 - tile.y %
/// tilingSize]` (`Floor.drawMain:266-268`): the `[col][row]` cell inside the
/// tiling sprite (rows count from its image top, matching `crop_uv`).
pub fn tiling_cell(x: i32, y: i32, size: u8) -> (u8, u8) {
    let size = size.max(1) as i32;
    let col = x.rem_euclid(size) as u8;
    let row = (size - 1 - y.rem_euclid(size)) as u8;
    (col, row)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `TileBitmask.values`: 256 entries, the 47 distinct sprite indices, and
    /// the Java edge pins (row 0 start, the fully-surrounded 13).
    #[test]
    fn bitmask_table_pins_the_47_region_indices() {
        assert_eq!(tile_bitmask::VALUES.len(), 256);
        let mut seen = [false; 47];
        for value in tile_bitmask::VALUES {
            assert!((value as usize) < 47, "index {value} out of range");
            seen[value as usize] = true;
        }
        assert!(seen.iter().all(|hit| *hit), "not all 47 indices are used");
        assert_eq!(tile_bitmask::value(0), 39);
        assert_eq!(tile_bitmask::value(255), 13);
        assert_eq!(tile_bitmask::value(5), 16);
    }

    #[test]
    fn autotile_bits_uses_geometry_d8_order() {
        assert_eq!(autotile_bits([false; 8]), 0);
        assert_eq!(autotile_bits([true; 8]), 0xff);
        let mut same = [false; 8];
        same[0] = true; // (1, 0)
        same[4] = true; // (-1, 0)
        assert_eq!(autotile_bits(same), 0b0001_0001);
    }

    #[test]
    fn autotile_region_names_match_floor_load() {
        // Plain autotile: `<name>-<bit>`.
        assert_eq!(
            autotile_region("metal-tiles-1", 39, 1, 0, 1, 0),
            "metal-tiles-1-39"
        );
        // Mid variants replace the bit-13 slice (`Floor.java:165-167`).
        assert_eq!(
            autotile_region("metal-tiles-7", 13, 1, 0, 9, 0),
            "metal-tiles-7-13"
        );
        assert_eq!(
            autotile_region("metal-tiles-7", 13, 1, 0, 9, 3),
            "metal-tiles-7-mid-4"
        );
        // Non-13 bits keep the normal slice even with mid variants.
        assert_eq!(
            autotile_region("metal-tiles-8", 27, 1, 0, 2, 1),
            "metal-tiles-8-27"
        );
        // `autotileVariants > 1`: `<name>-<v+1>-<bit>` (`TileBitmask.loadVariants`).
        assert_eq!(
            autotile_region("metal-tiles-11", 9, 3, 1, 1, 0),
            "metal-tiles-11-2-9"
        );
    }

    #[test]
    fn tiling_split_and_cell_match_java_split() {
        assert_eq!(tiling_split(256, 256), (8, 8));
        assert_eq!(tiling_split(32, 32), (1, 1));
        assert_eq!(tiling_split(0, 64), (1, 2));
        // `Floor.drawMain:266`: the variant is picked per block of `tilingSize`.
        assert_eq!(tiling_block(0, 0, 8), (0, 0));
        assert_eq!(tiling_block(9, 3, 8), (1, 0));
        assert_eq!(tiling_block(15, 17, 8), (1, 2));
        // `Floor.drawMain:266-268`: `[x % tilingSize][tilingSize - 1 - y % tilingSize]`.
        assert_eq!(tiling_cell(0, 0, 8), (0, 7));
        assert_eq!(tiling_cell(9, 3, 8), (1, 4));
        assert_eq!(tiling_cell(8, 8, 8), (0, 7));
        assert_eq!(tiling_cell(-1, -1, 8), (7, 0));
    }
}
