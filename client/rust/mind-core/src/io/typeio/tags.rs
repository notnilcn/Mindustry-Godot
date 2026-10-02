// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TypeIO` object type tags (plan 04 §6.3 — frozen table).
//!
//! Ported from `core/src/mindustry/io/TypeIO.java:43`. Values are parity ABI
//! (saves, network, schematics); never renumber.

/// Null/absent value.
pub const NULL: u8 = 0;
/// `i32`.
pub const INTEGER: u8 = 1;
/// `i64`.
pub const LONG: u8 = 2;
/// `f32`.
pub const FLOAT: u8 = 3;
/// String with an existence byte.
pub const STRING: u8 = 4;
/// Content reference (`u8` content type + `u16` id).
pub const CONTENT: u8 = 5;
/// Int sequence (`i16` count + `i32` values).
pub const INT_SEQ: u8 = 6;
/// Packed point (two `i32`).
pub const POINT2: u8 = 7;
/// Point array (`u8` count + packed `i32`).
pub const POINT2_ARRAY: u8 = 8;
/// Tech node (`u8` content type + `u16` id).
pub const TECH_NODE: u8 = 9;
/// Boolean.
pub const BOOLEAN: u8 = 10;
/// `f64`.
pub const DOUBLE: u8 = 11;
/// Building reference (`i32` packed tile pos).
pub const BUILDING: u8 = 12;
/// Logic access ordinal (`u16`).
pub const L_ACCESS: u8 = 13;
/// Byte array (`i32` count + bytes).
pub const BYTE_ARRAY: u8 = 14;
/// Legacy unit command (one byte, discarded).
pub const LEGACY: u8 = 15;
/// Boolean array (`i32` count + bytes).
pub const BOOL_ARRAY: u8 = 16;
/// Unit reference (`i32` entity id).
pub const UNIT: u8 = 17;
/// Vec2 array (`i16` count + `f32` pairs).
pub const VEC2_ARRAY: u8 = 18;
/// Vec2 (`f32` pair).
pub const VEC2: u8 = 19;
/// Team (`u8` id).
pub const TEAM: u8 = 20;
/// Int array (`i16` count + `i32` values).
pub const INT_ARRAY: u8 = 21;
/// Object array (`i32` count + tagged objects).
pub const OBJECT_ARRAY: u8 = 22;
/// Unit command (`u16` content id).
pub const UNIT_COMMAND: u8 = 23;
