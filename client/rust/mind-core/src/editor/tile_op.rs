// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `@Struct` tile-op packing (`editor/DrawOperation.java`, plan 19 §3.3).
//!
//! The generated `TileOp`/`TileOpData` `@Struct` layout is an ABI: `x`/`y` are
//! 14-bit fields, `type` is 3 bits and `value` is the remaining 33 bits,
//! truncated to `i32` exactly as upstream reads it back. `TileOpData` packs
//! three signed bytes little-endian into the low 24 bits of an `i32`.

/// `DrawOperation.opFloor`: previous floor content id.
pub const OP_FLOOR: u8 = 0;
/// `DrawOperation.opBlock`: previous block content id.
pub const OP_BLOCK: u8 = 1;
/// `DrawOperation.opRotation`: previous build rotation.
pub const OP_ROTATION: u8 = 2;
/// `DrawOperation.opTeam`: previous team id.
pub const OP_TEAM: u8 = 3;
/// `DrawOperation.opOverlay`: previous overlay content id.
pub const OP_OVERLAY: u8 = 4;
/// `DrawOperation.opData`: previous packed floor/overlay/data bytes.
pub const OP_DATA: u8 = 5;
/// `DrawOperation.opDataExtra`: previous extra data.
pub const OP_DATA_EXTRA: u8 = 6;

/// Packed tile op (`DrawOperation.TileOpStruct`). The inner `u64` is the wire
/// value handed around the editor; callers use the accessors below.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct TileOp(pub u64);

impl TileOp {
    /// `x`/`y` bit mask (14 bits).
    pub const AXIS_MASK: u64 = (1 << 14) - 1;
    /// `type` bit mask (3 bits).
    pub const TYPE_MASK: u64 = 0x7;
    /// `type` field offset.
    pub const TYPE_SHIFT: u32 = 28;
    /// `value` field offset.
    pub const VALUE_SHIFT: u32 = 31;

    /// Packs `(x, y, type, value)` (`TileOp.get`).
    pub fn get(x: i32, y: i32, ty: u8, value: i32) -> u64 {
        ((x as u64) & Self::AXIS_MASK)
            | (((y as u64) & Self::AXIS_MASK) << 14)
            | (((ty as u64) & Self::TYPE_MASK) << Self::TYPE_SHIFT)
            | ((value as u64) << Self::VALUE_SHIFT)
    }

    /// Tile x (`TileOp.x`).
    pub fn x(op: u64) -> i32 {
        (op & Self::AXIS_MASK) as i32
    }

    /// Tile y (`TileOp.y`).
    pub fn y(op: u64) -> i32 {
        ((op >> 14) & Self::AXIS_MASK) as i32
    }

    /// Op type (`TileOp.type`).
    pub fn ty(op: u64) -> u8 {
        ((op >> Self::TYPE_SHIFT) & Self::TYPE_MASK) as u8
    }

    /// Op value (`TileOp.value`); the 33-bit field truncates to `i32`.
    pub fn value(op: u64) -> i32 {
        (op >> Self::VALUE_SHIFT) as i32
    }
}

/// Three signed bytes packed little-endian (`DrawOperation.TileOpDataStruct`).
pub struct TileOpData;

impl TileOpData {
    /// Packs `(data, floor_data, overlay_data)` (`TileOpData.get`).
    pub fn get(data: i8, floor_data: i8, overlay_data: i8) -> i32 {
        (data as u8 as i32) | ((floor_data as u8 as i32) << 8) | ((overlay_data as u8 as i32) << 16)
    }

    /// Low byte (`TileOpData.data`).
    pub fn data(v: i32) -> i8 {
        (v & 0xFF) as u8 as i8
    }

    /// Middle byte (`TileOpData.floorData`).
    pub fn floor_data(v: i32) -> i8 {
        ((v >> 8) & 0xFF) as u8 as i8
    }

    /// High byte (`TileOpData.overlayData`).
    pub fn overlay_data(v: i32) -> i8 {
        ((v >> 16) & 0xFF) as u8 as i8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `editor::tests::tile_op_packing_roundtrip` (plan 19 §7a): all seven op
    /// kinds, the 14-bit axis boundary and signed value truncation.
    #[test]
    fn tile_op_packing_roundtrip() {
        for ty in OP_FLOOR..=OP_DATA_EXTRA {
            for (x, y) in [(0, 0), (1, 2), (13, 12), (0x3FFF, 0x3FFF)] {
                let value = 80i32;
                let op = TileOp::get(x, y, ty, value);
                assert_eq!(TileOp::x(op), x, "x ty={ty}");
                assert_eq!(TileOp::y(op), y, "y ty={ty}");
                assert_eq!(TileOp::ty(op), ty);
                assert_eq!(TileOp::value(op), value);
            }
        }
        // 14-bit boundary: 16383 is the largest representable axis value.
        let op = TileOp::get(16383, 16383, OP_BLOCK, 80);
        assert_eq!(TileOp::x(op), 16383);
        assert_eq!(TileOp::y(op), 16383);
        // Negative values round-trip through the 33-bit field's `i32` truncation.
        let neg = TileOp::get(3, 4, OP_DATA_EXTRA, -12345);
        assert_eq!(TileOp::value(neg), -12345);
        // x and y do not bleed into each other.
        let split = TileOp::get(0x2A, 0x15, OP_TEAM, 0);
        assert_eq!((TileOp::x(split), TileOp::y(split)), (0x2A, 0x15));
    }

    /// `editor::tests::tile_op_data_packing`: three signed bytes.
    #[test]
    fn tile_op_data_packing() {
        let packed = TileOpData::get(-1, 0x11, -2);
        assert_eq!(TileOpData::data(packed), -1);
        assert_eq!(TileOpData::floor_data(packed), 0x11);
        assert_eq!(TileOpData::overlay_data(packed), -2);
        assert_eq!(packed, 0x00FE_11FF);
    }
}
