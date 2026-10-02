// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Integer tile positions and Arc `Point2`-compatible packing.
//!
//! Ported from `core/src/mindustry/world/Tile.java` (`x`/`y` `short`) and Arc's
//! `arc.math.geom.Point2.pack`/`unpack` so `Tile::pos()` values are
//! interchangeable with plan 04/21 payloads (plan 06 §3.3).

/// Integer tile position (`Tile` coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct TilePos(pub i16, pub i16);

impl TilePos {
    /// The empty/detached tile sentinel used by carried build payloads
    /// (Java `emptyTile`; deviation plan 08 L2).
    pub const EMPTY: TilePos = TilePos(i16::MIN, i16::MIN);

    /// Creates a tile position.
    pub const fn new(x: i16, y: i16) -> Self {
        Self(x, y)
    }

    /// X coordinate.
    pub const fn x(self) -> i16 {
        self.0
    }

    /// Y coordinate.
    pub const fn y(self) -> i16 {
        self.1
    }

    /// Packs the position into one `i32` exactly like Arc `Point2.pack`
    /// (`(x & 0xffff) | ((y & 0xffff) << 16)`).
    pub const fn pack(self) -> i32 {
        pack(self.0, self.1)
    }

    /// Unpacks an Arc `Point2.pack` value.
    pub const fn from_pack(packed: i32) -> Self {
        let (x, y) = unpack(packed);
        Self(x, y)
    }

    /// The position relative to an origin (Arc `Point2` arithmetic).
    pub const fn relative_to(self, origin: TilePos) -> (i32, i32) {
        (
            self.0 as i32 - origin.0 as i32,
            self.1 as i32 - origin.1 as i32,
        )
    }

    /// Manhattan distance to another tile.
    pub const fn manhattan_to(self, other: TilePos) -> i32 {
        (self.0 as i32 - other.0 as i32).abs() + (self.1 as i32 - other.1 as i32).abs()
    }
}

/// Arc `Point2.pack`.
pub const fn pack(x: i16, y: i16) -> i32 {
    (x as u16 as i32) | ((y as u16 as i32) << 16)
}

/// Arc `Point2.unpack`.
pub const fn unpack(packed: i32) -> (i16, i16) {
    (packed as u16 as i16, ((packed >> 16) as u16) as i16)
}

/// A float point pair (`Point2`/`Vec2`-shaped) used by edge polygons.
pub const fn point(x: f32, y: f32) -> [f32; 2] {
    [x, y]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_roundtrip_matches_arc() {
        for (x, y) in [(0i16, 0i16), (1, 2), (-1, -2), (255, 511), (-32768, 32767)] {
            let pos = TilePos::new(x, y);
            assert_eq!(TilePos::from_pack(pos.pack()), pos);
            assert_eq!(pos.pack(), pack(x, y));
        }
        // Arc's exact bit layout for a positive sample.
        assert_eq!(TilePos::new(3, 4).pack(), 3 | (4 << 16));
    }
}
