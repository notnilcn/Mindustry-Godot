// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Tile-coordinate narrowing for the world/input glue.
//!
//! Upstream: none (port-only; the browser bridge accepted any JSON number as a tile).

#![allow(dead_code)] // adopted in W2 (docs/PLANS); C01 removes this

/// `(i16::try_from(x), i16::try_from(y))` as one Option; `None` if either is out of range.
pub fn tile_i16(x: impl TryInto<i16>, y: impl TryInto<i16>) -> Option<(i16, i16)> {
    Some((x.try_into().ok()?, y.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_i16_accepts_the_inclusive_i16_range() {
        assert_eq!(tile_i16(0i32, 0i32), Some((0, 0)));
        assert_eq!(tile_i16(-1i32, 1i32), Some((-1, 1)));
        assert_eq!(tile_i16(32_767i32, -32_768i32), Some((32_767, -32_768)));
        assert_eq!(tile_i16(32_768i32, 0i32), None);
        assert_eq!(tile_i16(0i32, -32_769i32), None);
    }
}
