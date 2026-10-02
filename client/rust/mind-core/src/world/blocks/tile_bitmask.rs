// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `core/src/mindustry/world/blocks/TileBitmask.java` (verbatim table +
// `load`/`loadVariants` region resolution).

//! 8-directional autotile bitmasks for 47-slice sprites (plan 03 §4 port map).
//!
//! `VALUES[mask]` maps an 8-neighbor bitmask to the slice index used by
//! `<name>-<i>` regions (see <https://github.com/GglLfr/tile-gen>). Consumed
//! by plans 08 (`Autotiler`) and 16 (floor renderer).

use crate::assets::atlas::{AtlasIndex, Region};

/// Autotile bitmasks for 8-directional sprites (`TileBitmask.values`).
#[rustfmt::skip]
pub const VALUES: [u8; 256] = [
    39, 36, 39, 36, 27, 16, 27, 24, 39, 36, 39, 36, 27, 16, 27, 24,
    38, 37, 38, 37, 17, 41, 17, 43, 38, 37, 38, 37, 26, 21, 26, 25,
    39, 36, 39, 36, 27, 16, 27, 24, 39, 36, 39, 36, 27, 16, 27, 24,
    38, 37, 38, 37, 17, 41, 17, 43, 38, 37, 38, 37, 26, 21, 26, 25,
    3,  4,  3,  4, 15, 40, 15, 20,  3,  4,  3,  4, 15, 40, 15, 20,
    5, 28,  5, 28, 29, 10, 29, 23,  5, 28,  5, 28, 31, 11, 31, 32,
    3,  4,  3,  4, 15, 40, 15, 20,  3,  4,  3,  4, 15, 40, 15, 20,
    2, 30,  2, 30,  9, 46,  9, 22,  2, 30,  2, 30, 14, 44, 14,  6,
    39, 36, 39, 36, 27, 16, 27, 24, 39, 36, 39, 36, 27, 16, 27, 24,
    38, 37, 38, 37, 17, 41, 17, 43, 38, 37, 38, 37, 26, 21, 26, 25,
    39, 36, 39, 36, 27, 16, 27, 24, 39, 36, 39, 36, 27, 16, 27, 24,
    38, 37, 38, 37, 17, 41, 17, 43, 38, 37, 38, 37, 26, 21, 26, 25,
    3,  0,  3,  0, 15, 42, 15, 12,  3,  0,  3,  0, 15, 42, 15, 12,
    5,  8,  5,  8, 29, 35, 29, 33,  5,  8,  5,  8, 31, 34, 31,  7,
    3,  0,  3,  0, 15, 42, 15, 12,  3,  0,  3,  0, 15, 42, 15, 12,
    2,  1,  2,  1,  9, 45,  9, 19,  2,  1,  2,  1, 14, 18, 14, 13,
];

/// Number of slices in a 47-slice autotile set.
pub const SLICE_COUNT: usize = 47;

/// `TileBitmask.load(name)` — resolves `<name>-<i>` for `i in 0..47`.
pub fn load<'a>(atlas: &'a AtlasIndex, name: &str) -> [Option<&'a Region>; SLICE_COUNT] {
    std::array::from_fn(|i| atlas.find(&format!("{name}-{i}")))
}

/// `TileBitmask.loadVariants(name, variants)` — resolves
/// `<name>-<v+1>-<i>` for each variant `v` and slice `i`.
pub fn load_variants<'a>(
    atlas: &'a AtlasIndex,
    name: &str,
    variants: usize,
) -> Vec<[Option<&'a Region>; SLICE_COUNT]> {
    (1..=variants)
        .map(|v| std::array::from_fn(|i| atlas.find(&format!("{name}-{v}-{i}"))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `tile_bitmask::tests::table_shape_and_ids` (plan 03 §7.1a): the table
    /// is 256 entries, every value is a valid slice index, and the upstream
    /// row pattern holds.
    #[test]
    fn table_shape_and_ids() {
        assert_eq!(VALUES.len(), 256);
        assert!(VALUES.iter().all(|&v| (v as usize) < SLICE_COUNT));
        // Spot-check upstream rows (first/last bytes of the Java literal).
        assert_eq!(
            &VALUES[0..16],
            &[
                39, 36, 39, 36, 27, 16, 27, 24, 39, 36, 39, 36, 27, 16, 27, 24
            ]
        );
        assert_eq!(VALUES[0], 39);
        assert_eq!(VALUES[255], 13);
        // Fully-surrounded tile blends to slice 39 (all-neighbors mask).
        assert_eq!(VALUES[0b1111_1111 & 0xff], 13);
    }

    #[test]
    fn load_resolves_slice_names() {
        let manifest = r#"{
  "format": 1, "pageCap": 4096, "fallback": false, "inputsHash": "x",
  "pages": [{"index": 0, "type": "environment", "file": "sprites.png", "width": 64, "height": 64, "sha256": "00"}],
  "regions": [
    {"name": "dirt-0", "page": 0, "x": 0, "y": 0, "w": 32, "h": 32, "offsets": [0,0], "original": [32,32], "pageType": "environment"},
    {"name": "dirt-46", "page": 0, "x": 0, "y": 32, "w": 32, "h": 32, "offsets": [0,0], "original": [32,32], "pageType": "environment"},
    {"name": "dirt-1-3", "page": 0, "x": 32, "y": 0, "w": 32, "h": 32, "offsets": [0,0], "original": [32,32], "pageType": "environment"}
  ]
}"#;
        let atlas = AtlasIndex::from_manifest_json(manifest).unwrap();
        let regions = load(&atlas, "dirt");
        assert!(regions[0].is_some());
        assert!(regions[46].is_some());
        assert!(regions[1].is_none());
        let variants = load_variants(&atlas, "dirt", 2);
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0][3].unwrap().name, "dirt-1-3");
        assert!(variants[1][3].is_none());
    }
}
