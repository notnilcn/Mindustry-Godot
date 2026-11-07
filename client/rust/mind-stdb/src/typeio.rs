// SPDX-License-Identifier: GPL-3.0-only

//! Network-only `TypeIO` codecs (GAPS #36 / PAR-0547).
//!
//! Ported from Mindustry `core/src/mindustry/io/TypeIO.java`: the writers and
//! readers the `@Remote` call packets need that the save/config object codec in
//! `mind-core::io::typeio` does not provide. Upstream list, in file order:
//! `writeMenuResult`/`readMenuResult`, `writeUnit`/`readUnit`,
//! `writePosEntity`/`readPosEntity`, `writeEntity`/`readEntity`,
//! `writeBuilding`/`readBuilding`, `writeTile`/`readTile`,
//! `writeController`/`readController`, `writeKick`/`readKick`,
//! `writeMarkerControl`/`readMarkerControl`, `writeAction`/`readAction`
//! (`Packets.AdminAction`), `writeSound`/`readSound`,
//! `writeTraceInfo`/`readTraceInfo`, `writeStringArray`/`readStringArray`,
//! `writeUiBuilder`/`readUiBuilder` (`mindustry.ui.builder.UiBuilder`), plus the
//! direct primitives those packets frame with (`writeString`, `writeStrings`,
//! `writeInts`, `writeShorts`, `writeBytes`, `writeEffect`, `writeColor`,
//! `writeBlock`, `writeTeam`).
//!
//! Invariants:
//! - Every `write_*` has an inverse `read_*`; tests round-trip each codec and
//!   reject malformed input (truncation, unknown discriminants, oversized
//!   arrays/strings, trailing bytes).
//! - Wire layout is big-endian Arc `Writes`/`Reads` framing: `b`/`ub` one byte,
//!   `s`/`us` two bytes, `i`/`l` four/eight, `f`/`d` IEEE-754, `bool` one byte,
//!   `str` an `i16` byte length followed by UTF-8 (`Writes.str`).
//! - Caps mirror upstream where it has them ([`MAX_ARRAY`], [`MAX_BYTE_ARRAY`],
//!   [`TRACE_LIST_LIMIT`], the `MenuResult.max*` constants) and add read-side
//!   caps where upstream is unbounded ([`MAX_WIRE_STRING`],
//!   [`MAX_MENU_OPTION_CELLS`], [`MAX_UI_ENTRIES`], [`MAX_UI_DEPTH`]).
//! - Entity references are raw ids (`UnitRef`, `BuildingPos`, `TilePos`,
//!   `EntityId`, ...) because `mind-stdb` owns no world; the caller resolves
//!   them through its entity index, exactly like `Groups.sync.getByID`.
//!
//! Not ported here (owned by the save codec or awaiting another subsystem):
//! `writePlans`/`writeClientPlans` (build plans, `mind-core::io::typeio`),
//! `writePayload`/`writeMounts`/`writeAbilities` (entity sync),
//! `writeRules`/`writeObjectives` (JSON codecs), and the tagged-object payload
//! of the two-argument `Call.effect` overload (`TypeIO.writeObject`).

use std::fmt;

use serde::{Deserialize, Serialize};

/// `TypeIO.maxArraySize` — generic array cap.
pub const MAX_ARRAY: usize = 1000;
/// `TypeIO.maxByteArraySize` — byte-array cap.
pub const MAX_BYTE_ARRAY: usize = 40_000;
/// Read-side cap on one UTF-8 string's byte length (upstream `readString` is
/// unbounded; this is the same 40k class as [`MAX_BYTE_ARRAY`]).
pub const MAX_WIRE_STRING: usize = 40_000;
/// `TypeIO.writeTraceInfo` caps both trace string arrays at 12 entries.
pub const TRACE_LIST_LIMIT: usize = 12;
/// `MenuResult.maxResultLen`.
pub const MENU_RESULT_MAX_RESULT_LEN: usize = 500;
/// `MenuResult.maxValueLen`.
pub const MENU_RESULT_MAX_VALUE_LEN: usize = 1000;
/// `MenuResult.maxTotalStringLen`.
pub const MENU_RESULT_MAX_TOTAL_STRING_LEN: usize = 6000;
/// `MenuResult.maxTotalValues`.
pub const MENU_RESULT_MAX_TOTAL_VALUES: usize = 500;
/// Read-side cap on one `String[][]` payload (`writeStringArray`).
pub const MAX_MENU_OPTION_CELLS: usize = 4096;
/// Read-side cap on entries per `NodeBuilder` (`writeUiBuilder`).
pub const MAX_UI_ENTRIES: usize = 512;
/// Read-side cap on nested `NodeBuilder` depth (upstream recurses unbounded).
pub const MAX_UI_DEPTH: usize = 16;
/// `Point2.pack(-1, -1)`: the `writeTile` null sentinel.
pub const NULL_TILE_POS: i32 = -1;

/// Codec failure. Every read is bounds-checked; malformed input is an error,
/// never a panic or a silently truncated value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TypeIoError {
    /// A read ran past the end of the buffer.
    #[error("unexpected end of input: needed {needed} byte(s), {remaining} left")]
    UnexpectedEof {
        /// Bytes the read needed.
        needed: usize,
        /// Bytes remaining in the buffer.
        remaining: usize,
    },
    /// A length-prefixed value exceeded its cap.
    #[error("{what} too large: {actual} (limit {limit})")]
    TooLarge {
        /// Value kind (`"array"`, `"string"`, ...).
        what: &'static str,
        /// Observed size.
        actual: usize,
        /// Enforced cap.
        limit: usize,
    },
    /// An enum ordinal / discriminant byte has no mapping (upstream would index
    /// out of bounds or recurse forever).
    #[error("unknown {what} discriminant: {tag}")]
    UnknownTag {
        /// Discriminant kind (`"kick reason"`, `"ui key"`, ...).
        what: &'static str,
        /// Offending byte.
        tag: u8,
    },
    /// A value was structurally valid but out of range.
    #[error("invalid {what}: {value}")]
    InvalidValue {
        /// Value kind.
        what: &'static str,
        /// Offending value.
        value: i64,
    },
    /// Bytes remained after a complete payload (upstream reads exactly the
    /// packet length, so a short body or extra bytes are both corruption).
    #[error("{trailing} trailing byte(s) after the payload")]
    TrailingBytes {
        /// Unread bytes.
        trailing: usize,
    },
    /// A structured payload nested deeper than [`MAX_UI_DEPTH`].
    #[error("payload nested deeper than {limit} levels")]
    DepthExceeded {
        /// Enforced depth.
        limit: usize,
    },
}

/// `Point2.pack`: `(x << 16) | (y & 0xFFFF)`.
pub const fn pack_point2(x: i32, y: i32) -> i32 {
    (x << 16) | (y & 0xFFFF)
}

/// `Point2.x(pack)`.
pub const fn unpack_point2_x(packed: i32) -> i32 {
    (packed >> 16) as i16 as i32
}

/// `Point2.y(pack)`.
pub const fn unpack_point2_y(packed: i32) -> i32 {
    packed as i16 as i32
}

// ---------------------------------------------------------------------------
// Wire primitives (Arc `Writes`/`Reads`).
// ---------------------------------------------------------------------------

/// Growing big-endian writer (`arc.util.io.Writes`).
#[derive(Debug, Default)]
pub struct TypeIoWriter {
    out: Vec<u8>,
}

impl TypeIoWriter {
    /// Empty writer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bytes written so far.
    pub fn pos(&self) -> usize {
        self.out.len()
    }

    /// Consumes the writer.
    pub fn into_bytes(self) -> Vec<u8> {
        self.out
    }

    /// Borrowed bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.out
    }

    /// `Writes.b` (`i8`).
    pub fn b(&mut self, value: i8) {
        self.out.push(value as u8);
    }

    /// Unsigned byte; identical bits to `Writes.b`.
    pub fn ub(&mut self, value: u8) {
        self.out.push(value);
    }

    /// `Writes.bool`.
    pub fn bool(&mut self, value: bool) {
        self.out.push(u8::from(value));
    }

    /// `Writes.s`.
    pub fn s(&mut self, value: i16) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    /// Unsigned short; identical bits to `Writes.s`.
    pub fn us(&mut self, value: u16) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    /// `Writes.i`.
    pub fn i(&mut self, value: i32) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    /// `Writes.l`.
    pub fn l(&mut self, value: i64) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    /// `Writes.f`.
    pub fn f(&mut self, value: f32) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    /// `Writes.d`.
    pub fn d(&mut self, value: f64) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    /// Raw bytes (`Writes.b(byte[])`).
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }

    /// `Writes.str`: `i16` byte length + UTF-8.
    pub fn str(&mut self, value: &str) -> Result<(), TypeIoError> {
        let bytes = value.as_bytes();
        if bytes.len() > MAX_WIRE_STRING {
            return Err(TypeIoError::TooLarge {
                what: "string",
                actual: bytes.len(),
                limit: MAX_WIRE_STRING,
            });
        }
        if bytes.len() > i16::MAX as usize {
            return Err(TypeIoError::TooLarge {
                what: "string",
                actual: bytes.len(),
                limit: i16::MAX as usize,
            });
        }
        self.s(bytes.len() as i16);
        self.bytes(bytes);
        Ok(())
    }
}

/// Bounds-checked big-endian reader (`arc.util.io.Reads`).
#[derive(Debug, Clone)]
pub struct TypeIoReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> TypeIoReader<'a> {
    /// Wraps one packet body.
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Current read offset.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Unread bytes.
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// Consumes the reader, failing when bytes remain.
    pub fn expect_end(self) -> Result<(), TypeIoError> {
        if self.remaining() > 0 {
            return Err(TypeIoError::TrailingBytes {
                trailing: self.remaining(),
            });
        }
        Ok(())
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], TypeIoError> {
        let remaining = self.remaining();
        if count > remaining {
            return Err(TypeIoError::UnexpectedEof {
                needed: count,
                remaining,
            });
        }
        let end = self.pos + count;
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    /// `Reads.b`.
    pub fn b(&mut self) -> Result<i8, TypeIoError> {
        Ok(self.take(1)?[0] as i8)
    }

    /// `Reads.ub`.
    pub fn ub(&mut self) -> Result<u8, TypeIoError> {
        Ok(self.take(1)?[0])
    }

    /// `Reads.bool`.
    pub fn bool(&mut self) -> Result<bool, TypeIoError> {
        Ok(self.take(1)?[0] != 0)
    }

    /// `Reads.s`.
    pub fn s(&mut self) -> Result<i16, TypeIoError> {
        let bytes = self.take(2)?;
        Ok(i16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// `Reads.us`.
    pub fn us(&mut self) -> Result<u16, TypeIoError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// `Reads.i`.
    pub fn i(&mut self) -> Result<i32, TypeIoError> {
        let bytes = self.take(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// `Reads.l`.
    pub fn l(&mut self) -> Result<i64, TypeIoError> {
        let bytes = self.take(8)?;
        Ok(i64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// `Reads.f`.
    pub fn f(&mut self) -> Result<f32, TypeIoError> {
        let bytes = self.take(4)?;
        Ok(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// `Reads.d`.
    pub fn d(&mut self) -> Result<f64, TypeIoError> {
        let bytes = self.take(8)?;
        Ok(f64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Raw bytes (`Reads.b(byte[])`).
    pub fn bytes(&mut self, count: usize) -> Result<&'a [u8], TypeIoError> {
        self.take(count)
    }

    /// `Reads.str`: `i16` byte length + UTF-8, capped at [`MAX_WIRE_STRING`].
    pub fn str(&mut self) -> Result<String, TypeIoError> {
        let len = self.s()?;
        if len < 0 {
            return Err(TypeIoError::InvalidValue {
                what: "string length",
                value: i64::from(len),
            });
        }
        let len = len as usize;
        if len > MAX_WIRE_STRING {
            return Err(TypeIoError::TooLarge {
                what: "string",
                actual: len,
                limit: MAX_WIRE_STRING,
            });
        }
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| TypeIoError::InvalidValue {
            what: "string (invalid UTF-8)",
            value: len as i64,
        })
    }

    /// `Reads.str(maxLen)`: as [`Self::str`], rejecting more than `max_chars`
    /// Unicode scalar values (Java's `String.length()` approximation).
    pub fn str_max(&mut self, max_chars: usize) -> Result<String, TypeIoError> {
        let value = self.str()?;
        if value.chars().count() > max_chars {
            return Err(TypeIoError::TooLarge {
                what: "string characters",
                actual: value.chars().count(),
                limit: max_chars,
            });
        }
        Ok(value)
    }
}

// ---------------------------------------------------------------------------
// TypeIO.writeString / writeStrings / writeInts / writeShorts / writeBytes.
// ---------------------------------------------------------------------------

/// `TypeIO.writeString(Writes, String)`: existence byte + `str` (`None` = null).
pub fn write_string(w: &mut TypeIoWriter, value: Option<&str>) -> Result<(), TypeIoError> {
    match value {
        Some(text) => {
            w.b(1);
            w.str(text)?;
        }
        None => w.b(0),
    }
    Ok(())
}

/// `TypeIO.readString(Writes)`.
pub fn read_string(r: &mut TypeIoReader<'_>) -> Result<Option<String>, TypeIoError> {
    if r.ub()? == 0 {
        return Ok(None);
    }
    Ok(Some(r.str()?))
}

/// `TypeIO.writeStrings(Writes, String[], int)`: `ub` count (capped) + strings.
pub fn write_strings(
    w: &mut TypeIoWriter,
    values: &[Option<String>],
    max_len: usize,
) -> Result<(), TypeIoError> {
    let used = values.len().min(max_len).min(u8::MAX as usize);
    w.ub(used as u8);
    for value in values.iter().take(used) {
        write_string(w, value.as_deref())?;
    }
    Ok(())
}

/// `TypeIO.readStrings` with a read-side entry cap.
pub fn read_strings(
    r: &mut TypeIoReader<'_>,
    limit: usize,
) -> Result<Vec<Option<String>>, TypeIoError> {
    let count = r.ub()? as usize;
    if count > limit {
        return Err(TypeIoError::TooLarge {
            what: "string array",
            actual: count,
            limit,
        });
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(read_string(r)?);
    }
    Ok(out)
}

/// `TypeIO.writeInts`: `s` count + `i32` values.
pub fn write_ints(w: &mut TypeIoWriter, values: &[i32]) -> Result<(), TypeIoError> {
    if values.len() > i16::MAX as usize {
        return Err(TypeIoError::TooLarge {
            what: "int array",
            actual: values.len(),
            limit: i16::MAX as usize,
        });
    }
    w.s(values.len() as i16);
    for value in values {
        w.i(*value);
    }
    Ok(())
}

/// `TypeIO.readInts`.
pub fn read_ints(r: &mut TypeIoReader<'_>) -> Result<Vec<i32>, TypeIoError> {
    let len = r.s()?;
    if len < 0 {
        return Err(TypeIoError::InvalidValue {
            what: "int array length",
            value: i64::from(len),
        });
    }
    let mut out = Vec::with_capacity(len as usize);
    for _ in 0..len {
        out.push(r.i()?);
    }
    Ok(out)
}

/// `TypeIO.writeShorts`: `s` count + `i16` values.
pub fn write_shorts(w: &mut TypeIoWriter, values: &[i16]) -> Result<(), TypeIoError> {
    if values.len() > i16::MAX as usize {
        return Err(TypeIoError::TooLarge {
            what: "short array",
            actual: values.len(),
            limit: i16::MAX as usize,
        });
    }
    w.s(values.len() as i16);
    for value in values {
        w.s(*value);
    }
    Ok(())
}

/// `TypeIO.readShorts`.
pub fn read_shorts(r: &mut TypeIoReader<'_>) -> Result<Vec<i16>, TypeIoError> {
    let len = r.s()?;
    if len < 0 {
        return Err(TypeIoError::InvalidValue {
            what: "short array length",
            value: i64::from(len),
        });
    }
    let mut out = Vec::with_capacity(len as usize);
    for _ in 0..len {
        out.push(r.s()?);
    }
    Ok(out)
}

/// `TypeIO.writeBytes`: `s` count + bytes, capped at [`MAX_BYTE_ARRAY`].
pub fn write_bytes(w: &mut TypeIoWriter, bytes: &[u8]) -> Result<(), TypeIoError> {
    if bytes.len() > MAX_BYTE_ARRAY {
        return Err(TypeIoError::TooLarge {
            what: "byte array",
            actual: bytes.len(),
            limit: MAX_BYTE_ARRAY,
        });
    }
    w.s(bytes.len() as i16);
    w.bytes(bytes);
    Ok(())
}

/// `TypeIO.readBytes`.
pub fn read_bytes(r: &mut TypeIoReader<'_>) -> Result<Vec<u8>, TypeIoError> {
    let len = r.s()?;
    if len < 0 {
        return Err(TypeIoError::InvalidValue {
            what: "byte array length",
            value: i64::from(len),
        });
    }
    let len = len as usize;
    if len > MAX_BYTE_ARRAY {
        return Err(TypeIoError::TooLarge {
            what: "byte array",
            actual: len,
            limit: MAX_BYTE_ARRAY,
        });
    }
    Ok(r.bytes(len)?.to_vec())
}

// ---------------------------------------------------------------------------
// Entity references.
// ---------------------------------------------------------------------------

/// `TypeIO.writeUnit`/`readUnit` payload (`Unit` or `UnitBox`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitRef {
    /// Type 2: a normal unit; `Groups.unit.getByID(id)`.
    Unit(i32),
    /// Type 1: a control-block unit; the payload is the packed tile position.
    Block(i32),
}

/// `TypeIO.writeUnit`: `b(type)` + `i(id|pos)`; `None` writes type 0.
pub fn write_unit(w: &mut TypeIoWriter, unit: Option<UnitRef>) {
    match unit {
        None => {
            w.b(0);
            w.i(0);
        }
        Some(UnitRef::Block(pos)) => {
            w.b(1);
            w.i(pos);
        }
        Some(UnitRef::Unit(id)) => {
            w.b(2);
            w.i(id);
        }
    }
}

/// `TypeIO.readUnit`.
pub fn read_unit(r: &mut TypeIoReader<'_>) -> Result<Option<UnitRef>, TypeIoError> {
    let kind = r.ub()?;
    let id = r.i()?;
    match kind {
        0 => Ok(None),
        1 => Ok(Some(UnitRef::Block(id))),
        2 => Ok(Some(UnitRef::Unit(id))),
        tag => Err(TypeIoError::UnknownTag {
            what: "unit ref",
            tag,
        }),
    }
}

/// Packed tile position (`Building.pos()` / `Tile.pos()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BuildingPos(pub i32);

/// `TypeIO.writeBuilding`: `i(pos)`, `-1` = null.
pub fn write_building(w: &mut TypeIoWriter, building: Option<BuildingPos>) {
    w.i(building.map_or(-1, |building| building.0));
}

/// `TypeIO.readBuilding` (`world.build(pos)`; resolution is the caller's job).
pub fn read_building(r: &mut TypeIoReader<'_>) -> Result<Option<BuildingPos>, TypeIoError> {
    let pos = r.i()?;
    Ok((pos != -1).then_some(BuildingPos(pos)))
}

/// `TypeIO.writeTile`: packed position, `Point2.pack(-1, -1)` = null.
pub fn write_tile(w: &mut TypeIoWriter, tile: Option<BuildingPos>) {
    w.i(tile.map_or(NULL_TILE_POS, |tile| tile.0));
}

/// `TypeIO.readTile` (`world.tile(pos)`; out-of-world resolution is the
/// caller's job).
pub fn read_tile(r: &mut TypeIoReader<'_>) -> Result<Option<BuildingPos>, TypeIoError> {
    let pos = r.i()?;
    Ok((pos != NULL_TILE_POS).then_some(BuildingPos(pos)))
}

/// `TypeIO.writePosEntity`: `i(id)`, `-1` = null (`Posc`).
pub fn write_pos_entity(w: &mut TypeIoWriter, entity: Option<i32>) {
    w.i(entity.unwrap_or(-1));
}

/// `TypeIO.readPosEntity` (`Groups.sync.getByID(id)`; raw id here).
pub fn read_pos_entity(r: &mut TypeIoReader<'_>) -> Result<Option<i32>, TypeIoError> {
    let id = r.i()?;
    Ok((id != -1).then_some(id))
}

/// `TypeIO.writeEntity`: `i(id)`, `-1` = null (`Entityc`).
pub fn write_entity(w: &mut TypeIoWriter, entity: Option<i32>) {
    w.i(entity.unwrap_or(-1));
}

/// `TypeIO.readEntity`.
pub fn read_entity(r: &mut TypeIoReader<'_>) -> Result<Option<i32>, TypeIoError> {
    let id = r.i()?;
    Ok((id != -1).then_some(id))
}

// ---------------------------------------------------------------------------
// Unit controllers (`TypeIO.writeController`/`readController`).
// ---------------------------------------------------------------------------

/// `CommandAI.attackTarget`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackTarget {
    /// A building, by packed position.
    Building(i32),
    /// A unit, by id.
    Unit(i32),
}

/// One `CommandAI.commandQueue` entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandQueueItem {
    /// Type 0: building, packed position.
    Building(i32),
    /// Type 1: unit id.
    Unit(i32),
    /// Type 2: world position.
    Position(f32, f32),
    /// Type 3: anything else (upstream writes 3 for garbage, skips it on read).
    Ignored,
}

/// Written `CommandAI` state (upstream controller type 9).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommandAi {
    /// `attackTarget`, when any.
    pub attack_target: Option<AttackTarget>,
    /// `targetPos`, when any.
    pub target_pos: Option<(f32, f32)>,
    /// `command.id`; `None` = null (written as `-1`).
    pub command: Option<i8>,
    /// `commandQueue`, in order.
    pub command_queue: Vec<CommandQueueItem>,
    /// Enabled stances (`UnitStance.id`s), in content order. One byte each
    /// (`TypeIO.writeStance`: `b(stance == null ? 255 : stance.id)`).
    pub stances: Vec<u8>,
}

/// `TypeIO.writeController`/`readController` payload (`UnitController`).
#[derive(Debug, Clone, PartialEq)]
pub enum UnitController {
    /// Type 0: a player controller (`Player.id`).
    Player {
        /// Player id.
        id: i32,
    },
    /// Type 1: a formation controller; upstream reads and returns `prev`.
    Formation {
        /// Formation id.
        id: i32,
    },
    /// Type 2: a plain AI controller (GroundAI and friends).
    Ai,
    /// Type 3: a `LogicAI` bound to a controller building position.
    Logic {
        /// Controlling building, packed position.
        controller: i32,
    },
    /// Type 5: an `AssemblerAI`.
    Assembler,
    /// Types 4/6/7/8/9: a `CommandAI` (upstream always writes type 9).
    Command(CommandAi),
}

impl UnitController {
    /// Wire type byte this controller is written as.
    pub fn wire_type(&self) -> u8 {
        match self {
            Self::Player { .. } => 0,
            Self::Formation { .. } => 1,
            Self::Ai => 2,
            Self::Logic { .. } => 3,
            Self::Assembler => 5,
            Self::Command(_) => 9,
        }
    }
}

/// `TypeIO.writeController`; command controllers are always written as the
/// current type 9 layout (upstream behaviour).
pub fn write_controller(w: &mut TypeIoWriter, controller: &UnitController) {
    w.ub(controller.wire_type());
    match controller {
        UnitController::Player { id } | UnitController::Formation { id } => w.i(*id),
        UnitController::Logic { controller } => w.i(*controller),
        UnitController::Ai | UnitController::Assembler => {}
        UnitController::Command(ai) => {
            w.bool(ai.attack_target.is_some());
            w.bool(ai.target_pos.is_some());
            if let Some((x, y)) = ai.target_pos {
                w.f(x);
                w.f(y);
            }
            if let Some(target) = ai.attack_target {
                match target {
                    AttackTarget::Building(pos) => {
                        w.b(1);
                        w.i(pos);
                    }
                    AttackTarget::Unit(id) => {
                        w.b(0);
                        w.i(id);
                    }
                }
            }
            w.b(ai.command.unwrap_or(-1));
            w.ub(ai.command_queue.len().min(u8::MAX as usize) as u8);
            for item in ai.command_queue.iter().take(u8::MAX as usize) {
                match item {
                    CommandQueueItem::Building(pos) => {
                        w.b(0);
                        w.i(*pos);
                    }
                    CommandQueueItem::Unit(id) => {
                        w.b(1);
                        w.i(*id);
                    }
                    CommandQueueItem::Position(x, y) => {
                        w.b(2);
                        w.f(*x);
                        w.f(*y);
                    }
                    CommandQueueItem::Ignored => w.b(3),
                }
            }
            w.ub(ai.stances.len().min(u8::MAX as usize) as u8);
            for stance in ai.stances.iter().take(u8::MAX as usize) {
                w.ub(*stance);
            }
        }
    }
}

/// `TypeIO.readController`. Type 1 returns [`UnitController::Formation`] where
/// upstream returns the previous controller; unknown bytes fall back to
/// [`UnitController::Ai`] (upstream's GroundAI/prev branch).
pub fn read_controller(r: &mut TypeIoReader<'_>) -> Result<UnitController, TypeIoError> {
    let ty = r.ub()?;
    match ty {
        0 => Ok(UnitController::Player { id: r.i()? }),
        1 => Ok(UnitController::Formation { id: r.i()? }),
        2 => Ok(UnitController::Ai),
        3 => Ok(UnitController::Logic { controller: r.i()? }),
        5 => Ok(UnitController::Assembler),
        4 | 6 | 7 | 8 | 9 => {
            let has_attack = r.bool()?;
            let has_pos = r.bool()?;
            let target_pos = if has_pos {
                Some((r.f()?, r.f()?))
            } else {
                None
            };
            let attack_target = if has_attack {
                let entity_type = r.b()?;
                let id = r.i()?;
                Some(if entity_type == 1 {
                    AttackTarget::Building(id)
                } else {
                    AttackTarget::Unit(id)
                })
            } else {
                None
            };
            let command = if ty >= 6 {
                Some(r.b()?).filter(|id| *id >= 0)
            } else {
                None
            };
            let mut command_queue = Vec::new();
            if ty >= 7 {
                let length = r.ub()? as usize;
                for _ in 0..length {
                    command_queue.push(match r.b()? {
                        0 => CommandQueueItem::Building(r.i()?),
                        1 => CommandQueueItem::Unit(r.i()?),
                        2 => CommandQueueItem::Position(r.f()?, r.f()?),
                        _ => CommandQueueItem::Ignored,
                    });
                }
            }
            let mut stances = Vec::new();
            if ty == 8 {
                stances.push(read_stance(r)?);
            } else if ty == 9 {
                let count = r.ub()? as usize;
                for _ in 0..count {
                    stances.push(read_stance(r)?);
                }
            }
            Ok(UnitController::Command(CommandAi {
                attack_target,
                target_pos,
                command,
                command_queue,
                stances,
            }))
        }
        _ => Ok(UnitController::Ai),
    }
}

/// `TypeIO.writeStance`: `b(stance == null ? 255 : stance.id)`.
pub fn write_stance(w: &mut TypeIoWriter, stance: Option<u8>) {
    w.ub(stance.unwrap_or(255));
}

/// `TypeIO.readStance`: `255` reads as `stop` (id 0); ids at or past the
/// content table are the caller's to clamp, exactly like upstream.
pub fn read_stance(r: &mut TypeIoReader<'_>) -> Result<u8, TypeIoError> {
    let value = r.ub()?;
    Ok(if value == 255 { 0 } else { value })
}

// ---------------------------------------------------------------------------
// KickReason / AdminAction / marker control / sound / effect / colour.
// ---------------------------------------------------------------------------

/// `Packets.KickReason` (`core/src/mindustry/net/Packets.java:17-42`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KickReason {
    /// `kick`.
    Kick,
    /// `clientOutdated`.
    ClientOutdated,
    /// `serverOutdated`.
    ServerOutdated,
    /// `banned`.
    Banned,
    /// `gameover` (quiet).
    Gameover,
    /// `recentKick`.
    RecentKick,
    /// `nameInUse`.
    NameInUse,
    /// `idInUse`.
    IdInUse,
    /// `nameEmpty`.
    NameEmpty,
    /// `customClient`.
    CustomClient,
    /// `serverClose`.
    ServerClose,
    /// `vote`.
    Vote,
    /// `typeMismatch`.
    TypeMismatch,
    /// `whitelist`.
    Whitelist,
    /// `playerLimit`.
    PlayerLimit,
    /// `serverRestarting`.
    ServerRestarting,
}

impl KickReason {
    /// `KickReason.all`, in ordinal order.
    pub const ALL: [Self; 16] = [
        Self::Kick,
        Self::ClientOutdated,
        Self::ServerOutdated,
        Self::Banned,
        Self::Gameover,
        Self::RecentKick,
        Self::NameInUse,
        Self::IdInUse,
        Self::NameEmpty,
        Self::CustomClient,
        Self::ServerClose,
        Self::Vote,
        Self::TypeMismatch,
        Self::Whitelist,
        Self::PlayerLimit,
        Self::ServerRestarting,
    ];

    /// Ordinal on the wire.
    pub fn ordinal(self) -> u8 {
        Self::ALL
            .iter()
            .position(|reason| *reason == self)
            .unwrap_or(0) as u8
    }

    /// Inverse of [`Self::ordinal`].
    pub fn from_ordinal(ordinal: u8) -> Option<Self> {
        Self::ALL.get(usize::from(ordinal)).copied()
    }

    /// `KickReason.quiet`: the client shows no dialog for `gameover`.
    pub fn quiet(self) -> bool {
        matches!(self, Self::Gameover)
    }

    /// Enum name, as used in `server.kicked.<name>` bundle keys.
    pub fn name(self) -> &'static str {
        match self {
            Self::Kick => "kick",
            Self::ClientOutdated => "clientOutdated",
            Self::ServerOutdated => "serverOutdated",
            Self::Banned => "banned",
            Self::Gameover => "gameover",
            Self::RecentKick => "recentKick",
            Self::NameInUse => "nameInUse",
            Self::IdInUse => "idInUse",
            Self::NameEmpty => "nameEmpty",
            Self::CustomClient => "customClient",
            Self::ServerClose => "serverClose",
            Self::Vote => "vote",
            Self::TypeMismatch => "typeMismatch",
            Self::Whitelist => "whitelist",
            Self::PlayerLimit => "playerLimit",
            Self::ServerRestarting => "serverRestarting",
        }
    }

    /// `KickReason.toString()`: the localized bundle key.
    pub fn bundle_key(self) -> String {
        format!("server.kicked.{}", self.name())
    }

    /// Inverse of [`Self::name`] (`KickReason.valueOf`): the console tokens
    /// `kick_all`/`kick_reason` producers accept (`"gameover"`,
    /// `"serverClose"`, …). Case-insensitive; unknown names return `None`.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|reason| reason.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// `TypeIO.writeKick`: ordinal byte.
pub fn write_kick(w: &mut TypeIoWriter, reason: KickReason) {
    w.ub(reason.ordinal());
}

/// `TypeIO.readKick`: ordinal byte, bounds-checked (`KickReason.all[read.b()]`).
pub fn read_kick(r: &mut TypeIoReader<'_>) -> Result<KickReason, TypeIoError> {
    let ordinal = r.ub()?;
    KickReason::from_ordinal(ordinal).ok_or(TypeIoError::UnknownTag {
        what: "kick reason",
        tag: ordinal,
    })
}

/// `Packets.AdminAction` (`kick`, `ban`, `trace`, `wave`, `switchTeam`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdminAction {
    /// `kick`.
    Kick,
    /// `ban`.
    Ban,
    /// `trace`.
    Trace,
    /// `wave`.
    Wave,
    /// `switchTeam`.
    SwitchTeam,
}

impl AdminAction {
    /// `AdminAction.all`, in ordinal order.
    pub const ALL: [Self; 5] = [
        Self::Kick,
        Self::Ban,
        Self::Trace,
        Self::Wave,
        Self::SwitchTeam,
    ];

    /// Ordinal on the wire.
    pub fn ordinal(self) -> u8 {
        Self::ALL
            .iter()
            .position(|action| *action == self)
            .unwrap_or(0) as u8
    }

    /// Inverse of [`Self::ordinal`].
    pub fn from_ordinal(ordinal: u8) -> Option<Self> {
        Self::ALL.get(usize::from(ordinal)).copied()
    }
}

/// `TypeIO.writeAction`: `AdminAction` ordinal byte.
pub fn write_action(w: &mut TypeIoWriter, action: AdminAction) {
    w.ub(action.ordinal());
}

/// `TypeIO.readAction`.
pub fn read_action(r: &mut TypeIoReader<'_>) -> Result<AdminAction, TypeIoError> {
    let ordinal = r.ub()?;
    AdminAction::from_ordinal(ordinal).ok_or(TypeIoError::UnknownTag {
        what: "admin action",
        tag: ordinal,
    })
}

/// `mindustry.logic.LMarkerControl` (`core/src/mindustry/logic/LMarkerControl.java`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkerControl {
    /// `remove`.
    Remove,
    /// `world`.
    World,
    /// `minimap`.
    Minimap,
    /// `light`.
    Light,
    /// `autoscale`.
    Autoscale,
    /// `pos`.
    Pos,
    /// `endPos`.
    EndPos,
    /// `drawLayer`.
    DrawLayer,
    /// `color`.
    Color,
    /// `radius`.
    Radius,
    /// `stroke`.
    Stroke,
    /// `outline`.
    Outline,
    /// `rotation`.
    Rotation,
    /// `shape`.
    Shape,
    /// `arc`.
    Arc,
    /// `flushText`.
    FlushText,
    /// `fontSize`.
    FontSize,
    /// `textHeight`.
    TextHeight,
    /// `textAlign`.
    TextAlign,
    /// `lineAlign`.
    LineAlign,
    /// `labelFlags`.
    LabelFlags,
    /// `texture`.
    Texture,
    /// `textureSize`.
    TextureSize,
    /// `posi`.
    Posi,
    /// `uvi`.
    Uvi,
    /// `colori`.
    Colori,
}

impl MarkerControl {
    /// `LMarkerControl.all`, in ordinal order.
    pub const ALL: [Self; 26] = [
        Self::Remove,
        Self::World,
        Self::Minimap,
        Self::Light,
        Self::Autoscale,
        Self::Pos,
        Self::EndPos,
        Self::DrawLayer,
        Self::Color,
        Self::Radius,
        Self::Stroke,
        Self::Outline,
        Self::Rotation,
        Self::Shape,
        Self::Arc,
        Self::FlushText,
        Self::FontSize,
        Self::TextHeight,
        Self::TextAlign,
        Self::LineAlign,
        Self::LabelFlags,
        Self::Texture,
        Self::TextureSize,
        Self::Posi,
        Self::Uvi,
        Self::Colori,
    ];

    /// Ordinal on the wire.
    pub fn ordinal(self) -> u8 {
        Self::ALL
            .iter()
            .position(|control| *control == self)
            .unwrap_or(0) as u8
    }

    /// Inverse of [`Self::ordinal`].
    pub fn from_ordinal(ordinal: u8) -> Option<Self> {
        Self::ALL.get(usize::from(ordinal)).copied()
    }

    /// Enum name (`LMarkerControl.name()`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Remove => "remove",
            Self::World => "world",
            Self::Minimap => "minimap",
            Self::Light => "light",
            Self::Autoscale => "autoscale",
            Self::Pos => "pos",
            Self::EndPos => "endPos",
            Self::DrawLayer => "drawLayer",
            Self::Color => "color",
            Self::Radius => "radius",
            Self::Stroke => "stroke",
            Self::Outline => "outline",
            Self::Rotation => "rotation",
            Self::Shape => "shape",
            Self::Arc => "arc",
            Self::FlushText => "flushText",
            Self::FontSize => "fontSize",
            Self::TextHeight => "textHeight",
            Self::TextAlign => "textAlign",
            Self::LineAlign => "lineAlign",
            Self::LabelFlags => "labelFlags",
            Self::Texture => "texture",
            Self::TextureSize => "textureSize",
            Self::Posi => "posi",
            Self::Uvi => "uvi",
            Self::Colori => "colori",
        }
    }
}

/// `TypeIO.writeMarkerControl`: `LMarkerControl` ordinal byte.
pub fn write_marker_control(w: &mut TypeIoWriter, control: MarkerControl) {
    w.ub(control.ordinal());
}

/// `TypeIO.readMarkerControl` (`LMarkerControl.all[read.ub()]`).
pub fn read_marker_control(r: &mut TypeIoReader<'_>) -> Result<MarkerControl, TypeIoError> {
    let ordinal = r.ub()?;
    MarkerControl::from_ordinal(ordinal).ok_or(TypeIoError::UnknownTag {
        what: "marker control",
        tag: ordinal,
    })
}

/// Standard `Sounds` constant id (`TypeIO.writeSound`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SoundId(pub i16);

/// `TypeIO.writeSound`: `s(Sounds.getSoundId(sound))`, `-1` for a non-standard
/// (modded) sound.
pub fn write_sound(w: &mut TypeIoWriter, sound: Option<SoundId>) {
    w.s(sound.map_or(-1, |sound| sound.0));
}

/// `TypeIO.readSound`: `Sounds.getSound(id)`; `-1` reads as `Sounds.none`.
pub fn read_sound(r: &mut TypeIoReader<'_>) -> Result<Option<SoundId>, TypeIoError> {
    let id = r.s()?;
    Ok((id >= 0).then_some(SoundId(id)))
}

/// Content id of an `Effect` (`TypeIO.writeEffect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EffectId(pub i16);

/// `TypeIO.writeEffect`: `s(effect.id)`.
pub fn write_effect(w: &mut TypeIoWriter, effect: Option<EffectId>) {
    w.s(effect.map_or(-1, |effect| effect.0));
}

/// `TypeIO.readEffect` (`Effect.get(read.us())`; `-1` = null).
pub fn read_effect(r: &mut TypeIoReader<'_>) -> Result<Option<EffectId>, TypeIoError> {
    let id = r.s()?;
    Ok((id >= 0).then_some(EffectId(id)))
}

/// Content id of a `Block` (`TypeIO.writeBlock`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub i16);

/// `TypeIO.writeBlock`: `s(block.id)`, `-1` = null.
pub fn write_block(w: &mut TypeIoWriter, block: Option<BlockId>) {
    w.s(block.map_or(-1, |block| block.0));
}

/// `TypeIO.readBlock`.
pub fn read_block(r: &mut TypeIoReader<'_>) -> Result<Option<BlockId>, TypeIoError> {
    let id = r.s()?;
    Ok((id >= 0).then_some(BlockId(id)))
}

/// `Team.id` (`TypeIO.writeTeam`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TeamId(pub u8);

/// `TypeIO.writeTeam`: `b(team == null ? 0 : team.id)`.
pub fn write_team(w: &mut TypeIoWriter, team: Option<TeamId>) {
    w.ub(team.map_or(0, |team| team.0));
}

/// `TypeIO.readTeam` (`Team.get(read.b())`).
pub fn read_team(r: &mut TypeIoReader<'_>) -> Result<TeamId, TypeIoError> {
    Ok(TeamId(r.ub()?))
}

/// `arc.graphics.Color.rgba()` bytes, RGBA8888 (`TypeIO.writeColor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha.
    pub a: u8,
}

impl Rgba {
    /// Packs as `Color.rgba()` (`r << 24 | g << 16 | b << 8 | a`).
    pub fn to_rgba(self) -> i32 {
        (i32::from(self.r) << 24)
            | (i32::from(self.g) << 16)
            | (i32::from(self.b) << 8)
            | i32::from(self.a)
    }

    /// Unpacks `Color.rgba()`.
    pub fn from_rgba(rgba: i32) -> Self {
        Self {
            r: (rgba >> 24) as u8,
            g: (rgba >> 16) as u8,
            b: (rgba >> 8) as u8,
            a: rgba as u8,
        }
    }
}

/// `TypeIO.writeColor`: `i(color.rgba())`.
pub fn write_color(w: &mut TypeIoWriter, color: Rgba) {
    w.i(color.to_rgba());
}

/// `TypeIO.readColor`.
pub fn read_color(r: &mut TypeIoReader<'_>) -> Result<Rgba, TypeIoError> {
    Ok(Rgba::from_rgba(r.i()?))
}

// ---------------------------------------------------------------------------
// TypeIO.writeObject / readObject (tagged objects, PAR-0578).
// ---------------------------------------------------------------------------
//
// Ported from `core/src/mindustry/io/TypeIO.java:38-320`: `writeObject`
// prepends one tag byte per value kind, `readObject` decodes it. The port has
// no live world/content registry here, so ids stay raw and the caller resolves
// them (`content`/`techNode` keep the content-type ordinal + id,
// `building`/`unit` keep the id); array/byte/string caps mirror the Java
// constants (a malformed body is an error, never a panic).

/// `TypeIO.maxArraySize` (`TypeIO.java:49`): tagged array entries.
pub const MAX_TAGGED_ARRAY: usize = 1000;
/// `TypeIO.maxByteArraySize` (`TypeIO.java:49`): tagged `byte[]` bytes.
pub const MAX_TAGGED_BYTES: usize = 40_000;
/// `readObjectSafe`'s string cap (`TypeIO.java:230`, `read.str(1200)`).
pub const MAX_TAGGED_STRING: usize = 1200;
/// Recursion cap for tagged object arrays (upstream rejects nested arrays
/// outright; the cap also bounds a hostile body).
pub const MAX_TAGGED_DEPTH: usize = 8;

/// `TypeIO.writeObject(Writes, Object)`: one arbitrary tagged value
/// (`TypeIO.java:72-190`).
///
/// Every upstream tag has a variant; the tags that name a live object keep the
/// raw id instead (`Building`/`Unit`), and `legacyType` reads back as
/// [`Self::Null`] like upstream. `Bullet`/`Seq` were written as `null`
/// upstream and have no variant.
#[derive(Debug, Clone, PartialEq)]
pub enum TaggedValue {
    /// Tag 0 (`nullType`).
    Null,
    /// Tag 1 (`integerType`).
    Int(i32),
    /// Tag 2 (`longType`).
    Long(i64),
    /// Tag 3 (`floatType`).
    Float(f32),
    /// Tag 4 (`stringType`).
    String(Option<String>),
    /// Tag 5 (`contentType`): `ContentType` ordinal + content id.
    Content {
        /// `ContentType` ordinal (`ContentType.all[...]`).
        content_type: u8,
        /// Content id.
        id: i16,
    },
    /// Tag 6 (`intSeqType`).
    IntSeq(Vec<i32>),
    /// Tag 7 (`point2Type`).
    Point2 {
        /// Point x.
        x: i32,
        /// Point y.
        y: i32,
    },
    /// Tag 8 (`point2ArrayType`), capped at 255 entries upstream.
    Point2Array(Vec<(i32, i32)>),
    /// Tag 9 (`techNodeType`): the unlocked content's type ordinal + id.
    TechNode {
        /// `ContentType` ordinal (`ContentType.all[...]`).
        content_type: u8,
        /// Content id.
        id: i16,
    },
    /// Tag 10 (`booleanType`).
    Boolean(bool),
    /// Tag 11 (`doubleType`).
    Double(f64),
    /// Tag 12 (`buildingType`): packed building position.
    Building(i32),
    /// Tag 13 (`lAccessType`): `LAccess` ordinal.
    LAccess(i16),
    /// Tag 14 (`byteArrayType`).
    Bytes(Vec<u8>),
    /// Tag 16 (`booleanArrayType`).
    Bools(Vec<bool>),
    /// Tag 17 (`unitType`): unit id.
    Unit(i32),
    /// Tag 18 (`vec2ArrayType`).
    Vec2Array(Vec<(f32, f32)>),
    /// Tag 19 (`vec2Type`).
    Vec2 {
        /// Vector x.
        x: f32,
        /// Vector y.
        y: f32,
    },
    /// Tag 20 (`teamType`): `Team.id`.
    Team(u8),
    /// Tag 21 (`intArrayType`).
    IntArray(Vec<i32>),
    /// Tag 22 (`objectArrayType`); nested arrays are rejected upstream.
    ObjectArray(Vec<TaggedValue>),
    /// Tag 23 (`unitCommandType`): unit-command content id.
    UnitCommand(u16),
}

fn tagged_array_len(len: usize, what: &'static str) -> Result<(), TypeIoError> {
    if len > MAX_TAGGED_ARRAY {
        return Err(TypeIoError::TooLarge {
            what,
            actual: len,
            limit: MAX_TAGGED_ARRAY,
        });
    }
    Ok(())
}

/// `TypeIO.writeObject(Writes, Object)`.
pub fn write_object(w: &mut TypeIoWriter, value: &TaggedValue) -> Result<(), TypeIoError> {
    match value {
        TaggedValue::Null => w.b(0),
        TaggedValue::Int(value) => {
            w.b(1);
            w.i(*value);
        }
        TaggedValue::Long(value) => {
            w.b(2);
            w.l(*value);
        }
        TaggedValue::Float(value) => {
            w.b(3);
            w.f(*value);
        }
        TaggedValue::String(value) => {
            w.b(4);
            write_string(w, value.as_deref())?;
        }
        TaggedValue::Content { content_type, id } => {
            w.b(5);
            w.b(*content_type as i8);
            w.s(*id);
        }
        TaggedValue::IntSeq(values) => {
            tagged_array_len(values.len(), "tagged int seq")?;
            w.b(6);
            w.s(values.len() as i16);
            for value in values {
                w.i(*value);
            }
        }
        TaggedValue::Point2 { x, y } => {
            w.b(7);
            w.i(*x);
            w.i(*y);
        }
        TaggedValue::Point2Array(points) => {
            if points.len() > u8::MAX as usize {
                return Err(TypeIoError::TooLarge {
                    what: "tagged point2 array",
                    actual: points.len(),
                    limit: u8::MAX as usize,
                });
            }
            w.b(8);
            w.b(points.len() as i8);
            for (x, y) in points {
                w.i(pack_point2(*x, *y));
            }
        }
        TaggedValue::TechNode { content_type, id } => {
            w.b(9);
            w.b(*content_type as i8);
            w.s(*id);
        }
        TaggedValue::Boolean(value) => {
            w.b(10);
            w.bool(*value);
        }
        TaggedValue::Double(value) => {
            w.b(11);
            w.d(*value);
        }
        TaggedValue::Building(pos) => {
            w.b(12);
            w.i(*pos);
        }
        TaggedValue::LAccess(ordinal) => {
            w.b(13);
            w.s(*ordinal);
        }
        TaggedValue::Bytes(bytes) => {
            if bytes.len() > MAX_TAGGED_BYTES {
                return Err(TypeIoError::TooLarge {
                    what: "tagged byte array",
                    actual: bytes.len(),
                    limit: MAX_TAGGED_BYTES,
                });
            }
            w.b(14);
            w.i(bytes.len() as i32);
            w.bytes(bytes);
        }
        TaggedValue::Bools(bools) => {
            tagged_array_len(bools.len(), "tagged boolean array")?;
            w.b(16);
            w.i(bools.len() as i32);
            for value in bools {
                w.bool(*value);
            }
        }
        TaggedValue::Unit(id) => {
            w.b(17);
            w.i(*id);
        }
        TaggedValue::Vec2Array(vectors) => {
            tagged_array_len(vectors.len(), "tagged vec2 array")?;
            w.b(18);
            w.s(vectors.len() as i16);
            for (x, y) in vectors {
                w.f(*x);
                w.f(*y);
            }
        }
        TaggedValue::Vec2 { x, y } => {
            w.b(19);
            w.f(*x);
            w.f(*y);
        }
        TaggedValue::Team(id) => {
            w.b(20);
            w.b(*id as i8);
        }
        TaggedValue::IntArray(values) => {
            tagged_array_len(values.len(), "tagged int array")?;
            w.b(21);
            write_ints(w, values)?;
        }
        TaggedValue::ObjectArray(values) => {
            tagged_array_len(values.len(), "tagged object array")?;
            w.b(22);
            w.i(values.len() as i32);
            for value in values {
                write_object(w, value)?;
            }
        }
        TaggedValue::UnitCommand(id) => {
            w.b(23);
            w.us(*id);
        }
    }
    Ok(())
}

/// `TypeIO.readObjectSafe` (`TypeIO.java:205-206, 218-320`): tag byte, safe
/// string cap, top-level arrays allowed; nested arrays are rejected.
pub fn read_object(r: &mut TypeIoReader<'_>) -> Result<TaggedValue, TypeIoError> {
    read_object_inner(r, true, 0)
}

fn read_object_inner(
    r: &mut TypeIoReader<'_>,
    allow_arrays: bool,
    depth: usize,
) -> Result<TaggedValue, TypeIoError> {
    if depth > MAX_TAGGED_DEPTH {
        return Err(TypeIoError::DepthExceeded {
            limit: MAX_TAGGED_DEPTH,
        });
    }
    let tag = r.ub()?;
    let array_allowed = |tag: u8| -> Result<(), TypeIoError> {
        if allow_arrays {
            Ok(())
        } else {
            Err(TypeIoError::InvalidValue {
                what: "nested tagged array",
                value: i64::from(tag),
            })
        }
    };
    match tag {
        0 => Ok(TaggedValue::Null),
        1 => Ok(TaggedValue::Int(r.i()?)),
        2 => Ok(TaggedValue::Long(r.l()?)),
        3 => Ok(TaggedValue::Float(r.f()?)),
        4 => {
            if r.b()? == 0 {
                Ok(TaggedValue::String(None))
            } else {
                Ok(TaggedValue::String(Some(r.str_max(MAX_TAGGED_STRING)?)))
            }
        }
        5 => Ok(TaggedValue::Content {
            content_type: r.ub()?,
            id: r.s()?,
        }),
        6 => {
            array_allowed(tag)?;
            let len = tagged_read_len(i32::from(r.s()?), "tagged int seq")?;
            let mut values = Vec::with_capacity(len);
            for _ in 0..len {
                values.push(r.i()?);
            }
            Ok(TaggedValue::IntSeq(values))
        }
        7 => Ok(TaggedValue::Point2 {
            x: r.i()?,
            y: r.i()?,
        }),
        8 => {
            array_allowed(tag)?;
            let len = usize::from(r.ub()?);
            let mut points = Vec::with_capacity(len);
            for _ in 0..len {
                let packed = r.i()?;
                points.push((unpack_point2_x(packed), unpack_point2_y(packed)));
            }
            Ok(TaggedValue::Point2Array(points))
        }
        9 => Ok(TaggedValue::TechNode {
            content_type: r.ub()?,
            id: r.s()?,
        }),
        10 => Ok(TaggedValue::Boolean(r.bool()?)),
        11 => Ok(TaggedValue::Double(r.d()?)),
        12 => Ok(TaggedValue::Building(r.i()?)),
        13 => Ok(TaggedValue::LAccess(r.s()?)),
        14 => {
            array_allowed(tag)?;
            let len = tagged_read_len(r.i()?, "tagged byte array")?;
            if len > MAX_TAGGED_BYTES {
                return Err(TypeIoError::TooLarge {
                    what: "tagged byte array",
                    actual: len,
                    limit: MAX_TAGGED_BYTES,
                });
            }
            Ok(TaggedValue::Bytes(r.bytes(len)?.to_vec()))
        }
        // `legacyType`: a discarded byte, read back as null upstream.
        15 => {
            let _ = r.b()?;
            Ok(TaggedValue::Null)
        }
        16 => {
            array_allowed(tag)?;
            let len = tagged_read_len(r.i()?, "tagged boolean array")?;
            let mut values = Vec::with_capacity(len.min(MAX_TAGGED_ARRAY));
            for _ in 0..len {
                values.push(r.bool()?);
            }
            Ok(TaggedValue::Bools(values))
        }
        17 => Ok(TaggedValue::Unit(r.i()?)),
        18 => {
            array_allowed(tag)?;
            let len = tagged_read_len(i32::from(r.s()?), "tagged vec2 array")?;
            let mut vectors = Vec::with_capacity(len);
            for _ in 0..len {
                vectors.push((r.f()?, r.f()?));
            }
            Ok(TaggedValue::Vec2Array(vectors))
        }
        19 => Ok(TaggedValue::Vec2 {
            x: r.f()?,
            y: r.f()?,
        }),
        20 => Ok(TaggedValue::Team(r.ub()?)),
        21 => {
            array_allowed(tag)?;
            Ok(TaggedValue::IntArray(read_ints(r)?))
        }
        22 => {
            array_allowed(tag)?;
            let len = tagged_read_len(r.i()?, "tagged object array")?;
            let mut values = Vec::with_capacity(len.min(MAX_TAGGED_ARRAY));
            for _ in 0..len {
                values.push(read_object_inner(r, false, depth + 1)?);
            }
            Ok(TaggedValue::ObjectArray(values))
        }
        23 => Ok(TaggedValue::UnitCommand(r.us()?)),
        other => Err(TypeIoError::UnknownTag {
            what: "tagged object",
            tag: other,
        }),
    }
}

/// Converts a signed array length, rejecting negatives and over-cap lengths.
fn tagged_read_len(len: i32, what: &'static str) -> Result<usize, TypeIoError> {
    let len = usize::try_from(len).map_err(|_| TypeIoError::InvalidValue {
        what,
        value: i64::from(len),
    })?;
    tagged_array_len(len, what)?;
    Ok(len)
}

// ---------------------------------------------------------------------------
// TraceInfo.
// ---------------------------------------------------------------------------

/// `Administration.TraceInfo`
/// (`core/src/mindustry/net/Administration.java:679-698`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceInfo {
    /// Last known IP.
    pub ip: Option<String>,
    /// Player UUID.
    pub uuid: Option<String>,
    /// Locale tag.
    pub locale: Option<String>,
    /// Whether the client had mods.
    pub modded: bool,
    /// Whether the client was mobile.
    pub mobile: bool,
    /// Session count.
    pub times_joined: i32,
    /// Kick count.
    pub times_kicked: i32,
    /// Known IPs, newest first (capped at [`TRACE_LIST_LIMIT`] on the wire).
    pub ips: Vec<Option<String>>,
    /// Known names, newest first (capped at [`TRACE_LIST_LIMIT`] on the wire).
    pub names: Vec<Option<String>>,
}

/// `TypeIO.writeTraceInfo`: strings, flags, counters, then both string arrays
/// capped at [`TRACE_LIST_LIMIT`].
pub fn write_trace_info(w: &mut TypeIoWriter, trace: &TraceInfo) -> Result<(), TypeIoError> {
    write_string(w, trace.ip.as_deref())?;
    write_string(w, trace.uuid.as_deref())?;
    write_string(w, trace.locale.as_deref())?;
    w.b(if trace.modded { 1 } else { 0 });
    w.b(if trace.mobile { 1 } else { 0 });
    w.i(trace.times_joined);
    w.i(trace.times_kicked);
    write_strings(w, &trace.ips, TRACE_LIST_LIMIT)?;
    write_strings(w, &trace.names, TRACE_LIST_LIMIT)
}

/// `TypeIO.readTraceInfo`; both list caps are enforced on read.
pub fn read_trace_info(r: &mut TypeIoReader<'_>) -> Result<TraceInfo, TypeIoError> {
    Ok(TraceInfo {
        ip: read_string(r)?,
        uuid: read_string(r)?,
        locale: read_string(r)?,
        modded: r.ub()? != 0,
        mobile: r.ub()? != 0,
        times_joined: r.i()?,
        times_kicked: r.i()?,
        ips: read_strings(r, TRACE_LIST_LIMIT)?,
        names: read_strings(r, TRACE_LIST_LIMIT)?,
    })
}

// ---------------------------------------------------------------------------
// Build plans, weapon mounts and ability data (PAR-0578).
// ---------------------------------------------------------------------------
//
// Ported from `core/src/mindustry/io/TypeIO.java`: `writePlan`/`readPlan`
// (`:645-680`), `writePlansQueueNet`/`readPlansQueueNet` (`:614-651`),
// `writePlans`/`readPlans` (`:685-711`), `writeClientPlans`/`readClientPlans`
// (`:716-772`), `writeMounts`/`readMounts` (`:393-427`) and
// `writeAbilities`/`readAbilities` (`:429-455`). These bodies are used by the
// entity/snapshot sync packets, not by the 23 ported `Call.*` kinds; they live
// here because they are `TypeIO` wire formats.
//
// `writePayload`/`readPayload` are deliberately not ported here: upstream
// delegates to `Payload.write/read` (`world/blocks/payloads/Payload.java`),
// which serializes a unit's `writeSync` (entity sync) or a building's items;
// that belongs with `mind-core`'s entity/save IO, not this crate.

/// `TypeIO.maxSyncedPlans` (`TypeIO.java:49`): plans per network sync.
pub const MAX_SYNCED_PLANS: usize = 20;
/// `TypeIO.getMaxPlans` config budget (`TypeIO.java:576-594`).
pub const MAX_SYNCED_PLAN_CONFIG_LEN: usize = 500;
/// `Vars.maxPlayerPreviewPlans` (`Vars.java:107`).
pub const MAX_PLAYER_PREVIEW_PLANS: usize = 1000;

/// One `mindustry.entities.units.BuildPlan`.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildPlan {
    /// Deconstruct instead of place (`plan.breaking`).
    pub breaking: bool,
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Block to place (`None` for a break plan / unknown id).
    pub block: Option<BlockId>,
    /// Placement rotation.
    pub rotation: u8,
    /// Block config (`writeObject`; [`TaggedValue::Null`] when absent).
    pub config: TaggedValue,
}

impl BuildPlan {
    /// A deconstruct plan (`BuildPlan(x, y)` with `breaking = true`).
    pub fn breaking(x: i32, y: i32) -> Self {
        Self {
            breaking: true,
            x,
            y,
            block: None,
            rotation: 0,
            config: TaggedValue::Null,
        }
    }
}

/// `TypeIO.writePlan`: break flag, packed position, then block/rotation/config
/// for a place plan.
pub fn write_plan(w: &mut TypeIoWriter, plan: &BuildPlan) -> Result<(), TypeIoError> {
    w.b(if plan.breaking { 1 } else { 0 });
    w.i(pack_point2(plan.x, plan.y));
    if !plan.breaking {
        w.s(plan.block.map_or(-1, |block| block.0));
        w.b(plan.rotation as i8);
        // Always has a config upstream (`write.b(1)`).
        w.b(1);
        write_object(w, &plan.config)?;
    }
    Ok(())
}

/// `TypeIO.readPlan`.
///
/// Upstream returns `null` when `world.tile(position) == null`; this codec has
/// no world, so the caller must validate the decoded position (the existing
/// `ClientPlan` path in `mind-core` does that).
pub fn read_plan(r: &mut TypeIoReader<'_>) -> Result<BuildPlan, TypeIoError> {
    let kind = r.b()?;
    let packed = r.i()?;
    let (x, y) = (unpack_point2_x(packed), unpack_point2_y(packed));
    if kind == 1 {
        return Ok(BuildPlan::breaking(x, y));
    }
    let block = r.s()?;
    let rotation = r.b()? as u8;
    let has_config = r.b()? == 1;
    let config = read_object(r)?;
    Ok(BuildPlan {
        breaking: false,
        x,
        y,
        block: (block >= 0).then_some(BlockId(block)),
        rotation,
        config: if has_config {
            config
        } else {
            TaggedValue::Null
        },
    })
}

/// `TypeIO.getMaxPlans`: at most [`MAX_SYNCED_PLANS`] plans, and it stops after
/// the [`MAX_SYNCED_PLAN_CONFIG_LEN`]-byte config budget.
pub fn max_synced_plans(plans: &[BuildPlan]) -> usize {
    let used = plans.len().min(MAX_SYNCED_PLANS);
    let mut total = 0usize;
    for (index, plan) in plans.iter().take(used).enumerate() {
        total = total.saturating_add(match &plan.config {
            TaggedValue::Bytes(bytes) => bytes.len(),
            TaggedValue::String(Some(text)) => text.len(),
            _ => 0,
        });
        if total > MAX_SYNCED_PLAN_CONFIG_LEN {
            return index + 1;
        }
    }
    used
}

/// `TypeIO.writePlansQueueNet`: `i(-1)` for null, else `i(used)` + plans.
pub fn write_plans_queue_net(
    w: &mut TypeIoWriter,
    plans: Option<&[BuildPlan]>,
) -> Result<(), TypeIoError> {
    let Some(plans) = plans else {
        w.i(-1);
        return Ok(());
    };
    let used = max_synced_plans(plans);
    w.i(used as i32);
    for plan in &plans[..used] {
        write_plan(w, plan)?;
    }
    Ok(())
}

/// `TypeIO.readPlansQueueNet`: capped at [`MAX_SYNCED_PLANS`].
pub fn read_plans_queue_net(
    r: &mut TypeIoReader<'_>,
) -> Result<Option<Vec<BuildPlan>>, TypeIoError> {
    let used = r.i()?;
    if used == -1 {
        return Ok(None);
    }
    let used = usize::try_from(used).map_err(|_| TypeIoError::InvalidValue {
        what: "plan queue length",
        value: i64::from(used),
    })?;
    if used > MAX_SYNCED_PLANS {
        return Err(TypeIoError::TooLarge {
            what: "plan queue",
            actual: used,
            limit: MAX_SYNCED_PLANS,
        });
    }
    let mut plans = Vec::with_capacity(used);
    for _ in 0..used {
        plans.push(read_plan(r)?);
    }
    Ok(Some(plans))
}

/// `TypeIO.writePlans` (`short` count, `-1` for null).
pub fn write_plans(w: &mut TypeIoWriter, plans: Option<&[BuildPlan]>) -> Result<(), TypeIoError> {
    let Some(plans) = plans else {
        w.s(-1);
        return Ok(());
    };
    if plans.len() > i16::MAX as usize {
        return Err(TypeIoError::TooLarge {
            what: "plan array",
            actual: plans.len(),
            limit: i16::MAX as usize,
        });
    }
    w.s(plans.len() as i16);
    for plan in plans {
        write_plan(w, plan)?;
    }
    Ok(())
}

/// `TypeIO.readPlans` (`short` count; `-1` = null).
pub fn read_plans(r: &mut TypeIoReader<'_>) -> Result<Option<Vec<BuildPlan>>, TypeIoError> {
    let count = r.s()?;
    if count == -1 {
        return Ok(None);
    }
    let count = usize::try_from(count).map_err(|_| TypeIoError::InvalidValue {
        what: "plan array length",
        value: i64::from(count),
    })?;
    let mut plans = Vec::with_capacity(count.min(MAX_PLAYER_PREVIEW_PLANS));
    for _ in 0..count {
        plans.push(read_plan(r)?);
    }
    Ok(Some(plans))
}

/// One client build-preview plan (`TypeIO.writeClientPlans`,
/// `TypeIO.ClientBuildPlans` entry).
///
/// The preview plan carries no breaking flag and only writes the rotation for
/// blocks with `Block.rotate` (content knowledge the caller supplies through
/// [`Self::rotates`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ClientPlan {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Block.
    pub block: BlockId,
    /// `Block.rotate`: whether the rotation byte is on the wire.
    pub rotates: bool,
    /// Placement rotation (ignored when `rotates` is false).
    pub rotation: u8,
    /// Config (`writeObject`; only null/number/boolean/content is valid).
    pub config: TaggedValue,
}

/// `TypeIO.validClientPlanConfig`: links (and other structures) are not sent
/// for preview plans, only `null`/numbers/booleans/content.
pub fn valid_client_plan_config(value: &TaggedValue) -> bool {
    matches!(
        value,
        TaggedValue::Null
            | TaggedValue::Int(_)
            | TaggedValue::Long(_)
            | TaggedValue::Float(_)
            | TaggedValue::Double(_)
            | TaggedValue::Boolean(_)
            | TaggedValue::Content { .. }
    )
}

/// `TypeIO.writeClientPlans`: `s(0)` for null/empty, else `s(count)` + plans.
pub fn write_client_plans(
    w: &mut TypeIoWriter,
    plans: Option<&[ClientPlan]>,
) -> Result<(), TypeIoError> {
    let Some(plans) = plans.filter(|plans| !plans.is_empty()) else {
        w.s(0);
        return Ok(());
    };
    if plans.len() > i16::MAX as usize {
        return Err(TypeIoError::TooLarge {
            what: "client plan array",
            actual: plans.len(),
            limit: i16::MAX as usize,
        });
    }
    w.s(plans.len() as i16);
    for plan in plans {
        w.i(pack_point2(plan.x, plan.y));
        w.s(plan.block.0);
        if plan.rotates {
            w.b(plan.rotation as i8);
        }
        let config = if valid_client_plan_config(&plan.config) {
            &plan.config
        } else {
            &TaggedValue::Null
        };
        write_object(w, config)?;
    }
    Ok(())
}

/// `TypeIO.readClientPlans`; `rotates` resolves `Block.rotate` for the block id
/// (content lives in `mind-core`, so the caller supplies it).
pub fn read_client_plans(
    r: &mut TypeIoReader<'_>,
    rotates: impl Fn(u16) -> bool,
) -> Result<Option<Vec<ClientPlan>>, TypeIoError> {
    let amount = r.s()?;
    if amount == 0 {
        return Ok(None);
    }
    let amount = usize::try_from(amount).map_err(|_| TypeIoError::InvalidValue {
        what: "client plan array length",
        value: i64::from(amount),
    })?;
    if amount > MAX_PLAYER_PREVIEW_PLANS {
        return Err(TypeIoError::TooLarge {
            what: "client plan array",
            actual: amount,
            limit: MAX_PLAYER_PREVIEW_PLANS,
        });
    }
    let mut plans = Vec::with_capacity(amount);
    for _ in 0..amount {
        // `writeClientPlans` writes `i(Point2.pack(x, y))`; upstream reads it
        // back as two unsigned shorts (`read.us()`), which is the same bytes.
        let x = i32::from(r.us()?);
        let y = i32::from(r.us()?);
        let block = r.us()?;
        let rotating = rotates(block);
        let rotation = if rotating { r.b()? as u8 } else { 0 };
        let config = read_client_plan_config(r)?;
        plans.push(ClientPlan {
            x,
            y,
            block: BlockId(block as i16),
            rotates: rotating,
            rotation,
            config,
        });
    }
    Ok(Some(plans))
}

/// `TypeIO.readClientPlanConfig`: the reduced config tag set (0, 1, 2, 3, 5,
/// 10, 11); every other tag is a rejection.
fn read_client_plan_config(r: &mut TypeIoReader<'_>) -> Result<TaggedValue, TypeIoError> {
    match r.ub()? {
        0 => Ok(TaggedValue::Null),
        1 => Ok(TaggedValue::Int(r.i()?)),
        2 => Ok(TaggedValue::Long(r.l()?)),
        3 => Ok(TaggedValue::Float(r.f()?)),
        5 => Ok(TaggedValue::Content {
            content_type: r.ub()?,
            id: r.s()?,
        }),
        10 => Ok(TaggedValue::Boolean(r.bool()?)),
        11 => Ok(TaggedValue::Double(r.d()?)),
        other => Err(TypeIoError::UnknownTag {
            what: "client plan config",
            tag: other,
        }),
    }
}

/// One weapon mount state (`TypeIO.writeMounts`, `TypeIO.java:393-401`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WeaponMount {
    /// `WeaponMount.shoot`.
    pub shoot: bool,
    /// `WeaponMount.rotate`.
    pub rotate: bool,
    /// Aim x.
    pub aim_x: f32,
    /// Aim y.
    pub aim_y: f32,
}

/// `TypeIO.writeMounts`: `b(count)`, then `b(state)` + two floats per mount.
pub fn write_mounts(w: &mut TypeIoWriter, mounts: &[WeaponMount]) -> Result<(), TypeIoError> {
    if mounts.len() > u8::MAX as usize {
        return Err(TypeIoError::TooLarge {
            what: "weapon mounts",
            actual: mounts.len(),
            limit: u8::MAX as usize,
        });
    }
    w.b(mounts.len() as i8);
    for mount in mounts {
        let state = (mount.shoot as u8) | ((mount.rotate as u8) << 1);
        w.b(state as i8);
        w.f(mount.aim_x);
        w.f(mount.aim_y);
    }
    Ok(())
}

/// `TypeIO.readMounts(Reads)` (allocation-free variant: returns the states).
pub fn read_mounts(r: &mut TypeIoReader<'_>) -> Result<Vec<WeaponMount>, TypeIoError> {
    let len = r.b()?;
    if len < 0 {
        return Ok(Vec::new());
    }
    let mut mounts = Vec::with_capacity(len as usize);
    for _ in 0..len {
        let state = r.b()? as u8;
        mounts.push(WeaponMount {
            shoot: state & 1 != 0,
            rotate: state & 2 != 0,
            aim_x: r.f()?,
            aim_y: r.f()?,
        });
    }
    Ok(mounts)
}

/// `TypeIO.writeAbilities`: `b(count)` + one float per ability.
pub fn write_abilities(w: &mut TypeIoWriter, abilities: &[f32]) -> Result<(), TypeIoError> {
    if abilities.len() > u8::MAX as usize {
        return Err(TypeIoError::TooLarge {
            what: "abilities",
            actual: abilities.len(),
            limit: u8::MAX as usize,
        });
    }
    w.b(abilities.len() as i8);
    for data in abilities {
        w.f(*data);
    }
    Ok(())
}

/// `TypeIO.readAbilities(Reads)`: `b(count)` + one float per ability.
pub fn read_abilities(r: &mut TypeIoReader<'_>) -> Result<Vec<f32>, TypeIoError> {
    let len = r.b()?;
    if len < 0 {
        return Ok(Vec::new());
    }
    let mut abilities = Vec::with_capacity(len as usize);
    for _ in 0..len {
        abilities.push(r.f()?);
    }
    Ok(abilities)
}

// ---------------------------------------------------------------------------
// String[][] (`TypeIO.writeStringArray`), used by `Call.menu`.
// ---------------------------------------------------------------------------

/// `TypeIO.writeStringArray`: `ub(rows)`, then per row `ub(columns)` + strings.
pub fn write_string_array(
    w: &mut TypeIoWriter,
    rows: &[Vec<Option<String>>],
) -> Result<(), TypeIoError> {
    if rows.len() > u8::MAX as usize {
        return Err(TypeIoError::TooLarge {
            what: "string array rows",
            actual: rows.len(),
            limit: u8::MAX as usize,
        });
    }
    w.ub(rows.len() as u8);
    for row in rows {
        if row.len() > u8::MAX as usize {
            return Err(TypeIoError::TooLarge {
                what: "string array columns",
                actual: row.len(),
                limit: u8::MAX as usize,
            });
        }
        w.ub(row.len() as u8);
        for cell in row {
            write_string(w, cell.as_deref())?;
        }
    }
    Ok(())
}

/// `TypeIO.readStringArray` with a total-cell cap ([`MAX_MENU_OPTION_CELLS`]).
pub fn read_string_array(
    r: &mut TypeIoReader<'_>,
) -> Result<Vec<Vec<Option<String>>>, TypeIoError> {
    let rows = r.ub()? as usize;
    let mut out = Vec::with_capacity(rows.min(MAX_MENU_OPTION_CELLS));
    let mut cells = 0usize;
    for _ in 0..rows {
        let columns = r.ub()? as usize;
        cells = cells.saturating_add(columns);
        if cells > MAX_MENU_OPTION_CELLS {
            return Err(TypeIoError::TooLarge {
                what: "string array cells",
                actual: cells,
                limit: MAX_MENU_OPTION_CELLS,
            });
        }
        let mut row = Vec::with_capacity(columns);
        for _ in 0..columns {
            row.push(read_string(r)?);
        }
        out.push(row);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// MenuResult (`TypeIO.writeMenuResult`/`readMenuResult`).
// ---------------------------------------------------------------------------

/// One `MenuResult.values` entry (`Float`, `Boolean` or `String`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MenuValue {
    /// Tag 0: slider value.
    Float(f32),
    /// Tag 1: checkbox state.
    Bool(bool),
    /// Tag 2: text field value.
    Str(String),
}

/// `mindustry.ui.builder.MenuResult`
/// (`core/src/mindustry/ui/builder/MenuResult.java:6-21`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MenuResult {
    /// Token passed to `menuBuilder`.
    pub token: i64,
    /// Clicked button result; `None` = cancelled (back button).
    pub result: Option<String>,
    /// Element id → value, in wire order.
    pub values: Vec<(String, MenuValue)>,
}

impl MenuResult {
    /// `MenuResult.wasCancelled`.
    pub fn was_cancelled(&self) -> bool {
        self.result.is_none()
    }

    /// `MenuResult.is(result)`.
    pub fn result_is(&self, expected: &str) -> bool {
        self.result.as_deref() == Some(expected)
    }

    /// Value for one element id.
    pub fn value(&self, key: &str) -> Option<&MenuValue> {
        self.values
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }
}

/// `TypeIO.writeMenuResult`: token, nullable result, value count, entries.
pub fn write_menu_result(w: &mut TypeIoWriter, result: &MenuResult) -> Result<(), TypeIoError> {
    w.l(result.token);
    write_string(w, result.result.as_deref())?;
    let used = result.values.len().min(MENU_RESULT_MAX_TOTAL_VALUES);
    w.us(used as u16);
    for (key, value) in result.values.iter().take(used) {
        w.str(key)?;
        match value {
            MenuValue::Float(number) => {
                w.b(0);
                w.f(*number);
            }
            MenuValue::Bool(flag) => {
                w.b(1);
                w.bool(*flag);
            }
            MenuValue::Str(text) => {
                w.b(2);
                w.str(text)?;
            }
        }
    }
    Ok(())
}

/// `TypeIO.readMenuResult`, with the `MenuResult.max*` caps enforced.
pub fn read_menu_result(r: &mut TypeIoReader<'_>) -> Result<MenuResult, TypeIoError> {
    let token = r.l()?;
    let mut remaining = MENU_RESULT_MAX_TOTAL_STRING_LEN;
    let result = match read_string(r)? {
        Some(text) => {
            if text.chars().count() > MENU_RESULT_MAX_RESULT_LEN {
                return Err(TypeIoError::TooLarge {
                    what: "menu result",
                    actual: text.chars().count(),
                    limit: MENU_RESULT_MAX_RESULT_LEN,
                });
            }
            remaining = remaining.saturating_sub(text.chars().count());
            Some(text)
        }
        None => None,
    };
    let entries = r.us()? as usize;
    if entries > MENU_RESULT_MAX_TOTAL_VALUES {
        return Err(TypeIoError::TooLarge {
            what: "menu result entries",
            actual: entries,
            limit: MENU_RESULT_MAX_TOTAL_VALUES,
        });
    }
    let mut values = Vec::with_capacity(entries);
    for _ in 0..entries {
        if remaining == 0 {
            return Err(TypeIoError::TooLarge {
                what: "menu result strings",
                actual: MENU_RESULT_MAX_TOTAL_STRING_LEN,
                limit: MENU_RESULT_MAX_TOTAL_STRING_LEN,
            });
        }
        let key = r.str()?;
        let key_len = key.chars().count();
        if key_len > remaining.min(MENU_RESULT_MAX_VALUE_LEN) {
            return Err(TypeIoError::TooLarge {
                what: "menu result key",
                actual: key_len,
                limit: remaining.min(MENU_RESULT_MAX_VALUE_LEN),
            });
        }
        remaining -= key_len;
        let value = match r.b()? {
            0 => MenuValue::Float(r.f()?),
            1 => MenuValue::Bool(r.bool()?),
            2 => {
                if remaining == 0 {
                    return Err(TypeIoError::TooLarge {
                        what: "menu result strings",
                        actual: MENU_RESULT_MAX_TOTAL_STRING_LEN,
                        limit: MENU_RESULT_MAX_TOTAL_STRING_LEN,
                    });
                }
                let text = r.str()?;
                let len = text.chars().count();
                if len > remaining.min(MENU_RESULT_MAX_VALUE_LEN) {
                    return Err(TypeIoError::TooLarge {
                        what: "menu result value",
                        actual: len,
                        limit: remaining.min(MENU_RESULT_MAX_VALUE_LEN),
                    });
                }
                remaining -= len;
                MenuValue::Str(text)
            }
            tag => {
                return Err(TypeIoError::UnknownTag {
                    what: "menu result value",
                    tag: tag as u8,
                });
            }
        };
        values.push((key, value));
    }
    Ok(MenuResult {
        token,
        result,
        values,
    })
}

// ---------------------------------------------------------------------------
// UiBuilder (`mindustry.ui.builder.UiBuilder`, `UiKey`).
// ---------------------------------------------------------------------------

/// `mindustry.ui.builder.UiKey` (`core/src/mindustry/ui/builder/UiKey.java`),
/// in ordinal order. The first fourteen entries are node types (`row` is the
/// last node-section key but is a cell marker, not a node).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum UiKey {
    /// `table` node.
    Table,
    /// `pane` node.
    Pane,
    /// `stack` node.
    Stack,
    /// `label` node.
    Label,
    /// `image` node.
    Image,
    /// `button` node.
    Button,
    /// `imageButton` node.
    ImageButton,
    /// `field` node.
    Field,
    /// `check` node.
    Check,
    /// `slider` node.
    Slider,
    /// `space` node.
    Space,
    /// `defaults` node.
    Defaults,
    /// `buttonTable` node.
    ButtonTable,
    /// `row` cell marker (boolean entry inside a container).
    Row,
    /// `text`.
    Text,
    /// `wrap`.
    Wrap,
    /// `region`.
    Region,
    /// `icon`.
    Icon,
    /// `placeholder`.
    Placeholder,
    /// `scaling`.
    Scaling,
    /// `background`.
    Background,
    /// `margin`.
    Margin,
    /// `id`.
    Id,
    /// `hint`.
    Hint,
    /// `maxLength`.
    MaxLength,
    /// `checked`.
    Checked,
    /// `min`.
    Min,
    /// `max`.
    Max,
    /// `step`.
    Step,
    /// `defaultValue`.
    DefaultValue,
    /// `clicked`.
    Clicked,
    /// `enter`.
    Enter,
    /// `style`.
    Style,
    /// `group`.
    Group,
    /// `condition`.
    Condition,
    /// `color`.
    Color,
    /// `disabled`.
    Disabled,
    /// `grow`.
    Grow,
    /// `growX`.
    GrowX,
    /// `growY`.
    GrowY,
    /// `fill`.
    Fill,
    /// `fillX`.
    FillX,
    /// `fillY`.
    FillY,
    /// `expand`.
    Expand,
    /// `expandX`.
    ExpandX,
    /// `expandY`.
    ExpandY,
    /// `width`.
    Width,
    /// `height`.
    Height,
    /// `size`.
    Size,
    /// `minWidth`.
    MinWidth,
    /// `maxWidth`.
    MaxWidth,
    /// `minHeight`.
    MinHeight,
    /// `maxHeight`.
    MaxHeight,
    /// `pad`.
    Pad,
    /// `padTop`.
    PadTop,
    /// `padLeft`.
    PadLeft,
    /// `padBottom`.
    PadBottom,
    /// `padRight`.
    PadRight,
    /// `align`.
    Align,
    /// `labelAlign`.
    LabelAlign,
    /// `colspan`.
    Colspan,
    /// `uniform`.
    Uniform,
    /// `uniformX`.
    UniformX,
    /// `uniformY`.
    UniformY,
}

impl UiKey {
    /// `UiKey.all`, in ordinal order.
    pub const ALL: [Self; 64] = [
        Self::Table,
        Self::Pane,
        Self::Stack,
        Self::Label,
        Self::Image,
        Self::Button,
        Self::ImageButton,
        Self::Field,
        Self::Check,
        Self::Slider,
        Self::Space,
        Self::Defaults,
        Self::ButtonTable,
        Self::Row,
        Self::Text,
        Self::Wrap,
        Self::Region,
        Self::Icon,
        Self::Placeholder,
        Self::Scaling,
        Self::Background,
        Self::Margin,
        Self::Id,
        Self::Hint,
        Self::MaxLength,
        Self::Checked,
        Self::Min,
        Self::Max,
        Self::Step,
        Self::DefaultValue,
        Self::Clicked,
        Self::Enter,
        Self::Style,
        Self::Group,
        Self::Condition,
        Self::Color,
        Self::Disabled,
        Self::Grow,
        Self::GrowX,
        Self::GrowY,
        Self::Fill,
        Self::FillX,
        Self::FillY,
        Self::Expand,
        Self::ExpandX,
        Self::ExpandY,
        Self::Width,
        Self::Height,
        Self::Size,
        Self::MinWidth,
        Self::MaxWidth,
        Self::MinHeight,
        Self::MaxHeight,
        Self::Pad,
        Self::PadTop,
        Self::PadLeft,
        Self::PadBottom,
        Self::PadRight,
        Self::Align,
        Self::LabelAlign,
        Self::Colspan,
        Self::Uniform,
        Self::UniformX,
        Self::UniformY,
    ];

    /// Node kinds `NodeBuilder.create` accepts (`UiBuilder.java:98-115`).
    pub const NODE_TYPES: [Self; 13] = [
        Self::Table,
        Self::Pane,
        Self::Stack,
        Self::Label,
        Self::Image,
        Self::Button,
        Self::ImageButton,
        Self::Field,
        Self::Check,
        Self::Slider,
        Self::Space,
        Self::Defaults,
        Self::ButtonTable,
    ];

    /// Ordinal on the wire.
    pub fn ordinal(self) -> u8 {
        self as u8
    }

    /// Inverse of [`Self::ordinal`].
    pub fn from_ordinal(ordinal: u8) -> Option<Self> {
        Self::ALL.get(usize::from(ordinal)).copied()
    }

    /// Whether this key names a `NodeBuilder` type.
    pub fn is_node_type(self) -> bool {
        Self::NODE_TYPES.contains(&self)
    }

    /// Enum name (`UiKey.name()`).
    pub fn name(self) -> &'static str {
        const NAMES: [&str; 64] = [
            "table",
            "pane",
            "stack",
            "label",
            "image",
            "button",
            "imageButton",
            "field",
            "check",
            "slider",
            "space",
            "defaults",
            "buttonTable",
            "row",
            "text",
            "wrap",
            "region",
            "icon",
            "placeholder",
            "scaling",
            "background",
            "margin",
            "id",
            "hint",
            "maxLength",
            "checked",
            "min",
            "max",
            "step",
            "defaultValue",
            "clicked",
            "enter",
            "style",
            "group",
            "condition",
            "color",
            "disabled",
            "grow",
            "growX",
            "growY",
            "fill",
            "fillX",
            "fillY",
            "expand",
            "expandX",
            "expandY",
            "width",
            "height",
            "size",
            "minWidth",
            "maxWidth",
            "minHeight",
            "maxHeight",
            "pad",
            "padTop",
            "padLeft",
            "padBottom",
            "padRight",
            "align",
            "labelAlign",
            "colspan",
            "uniform",
            "uniformX",
            "uniformY",
        ];
        NAMES[usize::from(self.ordinal())]
    }
}

/// One `NodeBuilder` entry value (`String`, `Float`, `Boolean` or a child node).
#[derive(Debug, Clone, PartialEq)]
pub enum UiValue {
    /// Tag 0.
    Str(String),
    /// Tag 1.
    Float(f32),
    /// Tag 2.
    Bool(bool),
    /// Tag 3: a nested builder (child node).
    Node(Box<UiNode>),
}

/// One `NodeBuilder` entry (property or child node).
#[derive(Debug, Clone, PartialEq)]
pub struct UiEntry {
    /// Entry key (`UiKey` ordinal on the wire).
    pub key: UiKey,
    /// Entry value.
    pub value: UiValue,
}

/// A serialized `UiBuilder.NodeBuilder`: node type + ordered entries.
#[derive(Debug, Clone, PartialEq)]
pub struct UiNode {
    /// Node type (`UiKey::NODE_TYPES`).
    pub kind: UiKey,
    /// Entries in insertion order.
    pub entries: Vec<UiEntry>,
}

impl UiNode {
    /// Empty node of `kind`.
    pub fn new(kind: UiKey) -> Self {
        Self {
            kind,
            entries: Vec::new(),
        }
    }

    /// Builder-style entry append.
    pub fn with(mut self, key: UiKey, value: UiValue) -> Self {
        self.entries.push(UiEntry { key, value });
        self
    }

    /// First string value for `key` (`NodeBuilder.str`).
    pub fn str_value(&self, key: UiKey) -> Option<&str> {
        self.entries.iter().find_map(|entry| match &entry.value {
            UiValue::Str(text) if entry.key == key => Some(text.as_str()),
            _ => None,
        })
    }

    /// First float value for `key` (`NodeBuilder.num`).
    pub fn num_value(&self, key: UiKey) -> Option<f32> {
        self.entries.iter().find_map(|entry| match entry.value {
            UiValue::Float(number) if entry.key == key => Some(number),
            _ => None,
        })
    }

    /// First boolean value for `key` (`NodeBuilder.bool`).
    pub fn bool_value(&self, key: UiKey) -> Option<bool> {
        self.entries.iter().find_map(|entry| match entry.value {
            UiValue::Bool(flag) if entry.key == key => Some(flag),
            _ => None,
        })
    }
}

/// `TypeIO.writeUiBuilder` (`NodeBuilder.write`): type ordinal, `s` entry
/// count, entries.
pub fn write_ui_builder(w: &mut TypeIoWriter, node: &UiNode) -> Result<(), TypeIoError> {
    if !node.kind.is_node_type() {
        return Err(TypeIoError::InvalidValue {
            what: "ui node type",
            value: i64::from(node.kind.ordinal()),
        });
    }
    if node.entries.len() > i16::MAX as usize {
        return Err(TypeIoError::TooLarge {
            what: "ui node entries",
            actual: node.entries.len(),
            limit: i16::MAX as usize,
        });
    }
    w.ub(node.kind.ordinal());
    w.s(node.entries.len() as i16);
    for entry in &node.entries {
        w.s(i16::from(entry.key.ordinal()));
        match &entry.value {
            UiValue::Str(text) => {
                w.b(0);
                w.str(text)?;
            }
            UiValue::Float(number) => {
                w.b(1);
                w.f(*number);
            }
            UiValue::Bool(flag) => {
                w.b(2);
                w.bool(*flag);
            }
            UiValue::Node(child) => {
                w.b(3);
                write_ui_builder(w, child)?;
            }
        }
    }
    Ok(())
}

/// `TypeIO.readUiBuilder` (`NodeBuilder.read`), depth/size capped.
pub fn read_ui_builder(r: &mut TypeIoReader<'_>) -> Result<UiNode, TypeIoError> {
    read_ui_node(r, 0)
}

fn read_ui_node(r: &mut TypeIoReader<'_>, depth: usize) -> Result<UiNode, TypeIoError> {
    if depth > MAX_UI_DEPTH {
        return Err(TypeIoError::DepthExceeded {
            limit: MAX_UI_DEPTH,
        });
    }
    let ordinal = r.ub()?;
    let kind = UiKey::from_ordinal(ordinal).ok_or(TypeIoError::UnknownTag {
        what: "ui key",
        tag: ordinal,
    })?;
    if !kind.is_node_type() {
        return Err(TypeIoError::InvalidValue {
            what: "ui node type",
            value: i64::from(ordinal),
        });
    }
    let count = r.us()? as usize;
    if count > MAX_UI_ENTRIES {
        return Err(TypeIoError::TooLarge {
            what: "ui node entries",
            actual: count,
            limit: MAX_UI_ENTRIES,
        });
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let key_ordinal = r.us()?;
        let key = u8::try_from(key_ordinal)
            .ok()
            .and_then(UiKey::from_ordinal)
            .ok_or(TypeIoError::UnknownTag {
                what: "ui entry key",
                tag: key_ordinal.min(255) as u8,
            })?;
        let value = match r.ub()? {
            0 => UiValue::Str(r.str()?),
            1 => UiValue::Float(r.f()?),
            2 => UiValue::Bool(r.bool()?),
            3 => UiValue::Node(Box::new(read_ui_node(r, depth + 1)?)),
            tag => {
                return Err(TypeIoError::UnknownTag {
                    what: "ui entry value",
                    tag,
                });
            }
        };
        entries.push(UiEntry { key, value });
    }
    Ok(UiNode { kind, entries })
}

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "#{:02x}{:02x}{:02x}{:02x}",
            self.r, self.g, self.b, self.a
        )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn round_trip<T>(
        write: impl Fn(&mut TypeIoWriter, &T),
        read: impl Fn(&mut TypeIoReader<'_>) -> Result<T, TypeIoError>,
        value: T,
    ) -> T {
        let mut writer = TypeIoWriter::new();
        write(&mut writer, &value);
        let bytes = writer.into_bytes();
        let mut reader = TypeIoReader::new(&bytes);
        let decoded = read(&mut reader).expect("decode");
        reader.expect_end().expect("no trailing bytes");
        decoded
    }

    #[test]
    fn primitives_round_trip_and_bound_strings() {
        let mut writer = TypeIoWriter::new();
        writer.b(-7);
        writer.ub(250);
        writer.bool(true);
        writer.s(-1234);
        writer.us(65000);
        writer.i(-2_000_000);
        writer.l(9_000_000_000_000);
        writer.f(1.25);
        writer.d(-2.5);
        writer.str("héllo").unwrap();
        writer.bytes(&[1, 2, 3]);
        let bytes = writer.into_bytes();

        let mut reader = TypeIoReader::new(&bytes);
        assert_eq!(reader.b().unwrap(), -7);
        assert_eq!(reader.ub().unwrap(), 250);
        assert!(reader.bool().unwrap());
        assert_eq!(reader.s().unwrap(), -1234);
        assert_eq!(reader.us().unwrap(), 65000);
        assert_eq!(reader.i().unwrap(), -2_000_000);
        assert_eq!(reader.l().unwrap(), 9_000_000_000_000);
        assert_eq!(reader.f().unwrap(), 1.25);
        assert_eq!(reader.d().unwrap(), -2.5);
        assert_eq!(reader.str().unwrap(), "héllo");
        assert_eq!(reader.bytes(3).unwrap(), &[1, 2, 3]);
        reader.expect_end().unwrap();

        // Truncation is an error, never a panic.
        let mut short = TypeIoReader::new(&bytes[..bytes.len() - 1]);
        assert!(matches!(
            short.bytes(bytes.len()),
            Err(TypeIoError::UnexpectedEof { .. })
        ));

        // Negative and truncated string lengths are rejected.
        let mut negative = TypeIoWriter::new();
        negative.s(-1);
        assert!(matches!(
            TypeIoReader::new(negative.as_bytes()).str(),
            Err(TypeIoError::InvalidValue {
                what: "string length",
                ..
            })
        ));
        let mut truncated = TypeIoWriter::new();
        truncated.s(16);
        truncated.bytes(b"short");
        assert!(matches!(
            TypeIoReader::new(truncated.as_bytes()).str(),
            Err(TypeIoError::UnexpectedEof { .. })
        ));
    }

    #[test]
    fn str_too_long_is_rejected() {
        let long = "x".repeat(MAX_WIRE_STRING + 1);
        let mut writer = TypeIoWriter::new();
        assert!(matches!(
            writer.str(&long),
            Err(TypeIoError::TooLarge { .. })
        ));
    }

    #[test]
    fn strings_ints_shorts_bytes_round_trip() {
        let strings = vec![Some("alpha".to_string()), None, Some("γ".to_string())];
        let mut writer = TypeIoWriter::new();
        write_strings(&mut writer, &strings, TRACE_LIST_LIMIT).unwrap();
        let mut reader = TypeIoReader::new(writer.as_bytes());
        assert_eq!(
            read_strings(&mut reader, TRACE_LIST_LIMIT).unwrap(),
            strings
        );
        reader.expect_end().unwrap();

        // The write cap truncates; the read cap rejects.
        let many: Vec<Option<String>> = (0..20).map(|i| Some(i.to_string())).collect();
        let mut writer = TypeIoWriter::new();
        write_strings(&mut writer, &many, 5).unwrap();
        let mut reader = TypeIoReader::new(writer.as_bytes());
        assert_eq!(read_strings(&mut reader, 5).unwrap().len(), 5);
        let mut reader = TypeIoReader::new(writer.as_bytes());
        assert!(matches!(
            read_strings(&mut reader, 4),
            Err(TypeIoError::TooLarge { .. })
        ));

        let ints = vec![1, -2, 3];
        let shorts = vec![4i16, -5];
        let mut writer = TypeIoWriter::new();
        write_ints(&mut writer, &ints).unwrap();
        write_shorts(&mut writer, &shorts).unwrap();
        write_bytes(&mut writer, &[9, 8, 7]).unwrap();
        let mut reader = TypeIoReader::new(writer.as_bytes());
        assert_eq!(read_ints(&mut reader).unwrap(), ints);
        assert_eq!(read_shorts(&mut reader).unwrap(), shorts);
        assert_eq!(read_bytes(&mut reader).unwrap(), vec![9, 8, 7]);
        reader.expect_end().unwrap();
    }

    #[test]
    fn string_and_entity_codecs_round_trip() {
        // Nullable strings.
        let mut writer = TypeIoWriter::new();
        write_string(&mut writer, Some("a")).unwrap();
        write_string(&mut writer, None).unwrap();
        let mut reader = TypeIoReader::new(writer.as_bytes());
        assert_eq!(read_string(&mut reader).unwrap().as_deref(), Some("a"));
        assert_eq!(read_string(&mut reader).unwrap(), None);

        // Unit refs: standard, block and null.
        for unit in [
            None,
            Some(UnitRef::Unit(77)),
            Some(UnitRef::Block(pack_point2(3, 4))),
        ] {
            assert_eq!(
                round_trip(|w, unit| write_unit(w, *unit), read_unit, unit),
                unit
            );
        }

        // Building / tile / entity / pos-entity null sentinels.
        for building in [None, Some(BuildingPos(pack_point2(5, 6)))] {
            assert_eq!(
                round_trip(
                    |w, value| write_building(w, *value),
                    read_building,
                    building
                ),
                building
            );
        }
        for tile in [None, Some(BuildingPos(pack_point2(7, 8)))] {
            assert_eq!(
                round_trip(|w, value| write_tile(w, *value), read_tile, tile),
                tile
            );
        }
        assert_eq!(
            round_trip(|w, value| write_entity(w, *value), read_entity, None),
            None
        );
        assert_eq!(
            round_trip(|w, value| write_entity(w, *value), read_entity, Some(42)),
            Some(42)
        );
        assert_eq!(
            round_trip(
                |w, value| write_pos_entity(w, *value),
                read_pos_entity,
                Some(9)
            ),
            Some(9)
        );

        // Unknown unit tag.
        let mut writer = TypeIoWriter::new();
        writer.b(7);
        writer.i(1);
        assert!(matches!(
            read_unit(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::UnknownTag {
                what: "unit ref",
                tag: 7
            })
        ));
    }

    #[test]
    fn controllers_round_trip_every_variant() {
        let controllers = vec![
            UnitController::Player { id: 3 },
            UnitController::Formation { id: 8 },
            UnitController::Ai,
            UnitController::Logic { controller: 1234 },
            UnitController::Assembler,
            UnitController::Command(CommandAi::default()),
            UnitController::Command(CommandAi {
                attack_target: Some(AttackTarget::Building(pack_point2(2, 3))),
                target_pos: Some((10.5, -4.25)),
                command: Some(2),
                command_queue: vec![
                    CommandQueueItem::Building(16),
                    CommandQueueItem::Unit(5),
                    CommandQueueItem::Position(1.0, 2.0),
                    CommandQueueItem::Ignored,
                ],
                stances: vec![1, 3],
            }),
            UnitController::Command(CommandAi {
                attack_target: Some(AttackTarget::Unit(99)),
                ..CommandAi::default()
            }),
        ];
        for controller in controllers {
            assert_eq!(
                round_trip(write_controller, read_controller, controller.clone()),
                controller
            );
        }

        // Legacy types 4/6/7/8 decode into the same CommandAI shape.
        let mut writer = TypeIoWriter::new();
        writer.b(6);
        writer.bool(false);
        writer.bool(false);
        writer.b(1);
        let decoded = read_controller(&mut TypeIoReader::new(writer.as_bytes())).unwrap();
        assert_eq!(
            decoded,
            UnitController::Command(CommandAi {
                command: Some(1),
                ..CommandAi::default()
            })
        );

        // Type 9 with two stances plus a byte-level stance round trip.
        let mut writer = TypeIoWriter::new();
        writer.b(9);
        writer.bool(false);
        writer.bool(false);
        writer.b(2);
        writer.ub(0); // empty command queue
        writer.ub(2); // two stances
        writer.ub(1);
        writer.ub(3);
        assert_eq!(
            read_controller(&mut TypeIoReader::new(writer.as_bytes())).unwrap(),
            UnitController::Command(CommandAi {
                command: Some(2),
                stances: vec![1, 3],
                ..CommandAi::default()
            })
        );
        let mut writer = TypeIoWriter::new();
        write_stance(&mut writer, None);
        write_stance(&mut writer, Some(4));
        let mut reader = TypeIoReader::new(writer.as_bytes());
        assert_eq!(read_stance(&mut reader).unwrap(), 0);
        assert_eq!(read_stance(&mut reader).unwrap(), 4);
        reader.expect_end().unwrap();

        // Unknown controller bytes fall back to plain AI (upstream else-branch).
        let mut writer = TypeIoWriter::new();
        writer.b(12);
        assert_eq!(
            read_controller(&mut TypeIoReader::new(writer.as_bytes())).unwrap(),
            UnitController::Ai
        );
    }

    #[test]
    fn kick_reason_marker_and_action_cover_every_variant() {
        for reason in KickReason::ALL {
            assert_eq!(
                round_trip(|w, value| write_kick(w, *value), read_kick, reason),
                reason
            );
            assert_eq!(KickReason::from_ordinal(reason.ordinal()), Some(reason));
            assert!(!reason.name().is_empty());
            assert_eq!(
                reason.bundle_key(),
                format!("server.kicked.{}", reason.name())
            );
        }
        assert!(KickReason::Gameover.quiet());
        assert!(!KickReason::Kick.quiet());
        assert_eq!(KickReason::from_ordinal(16), None);
        let mut writer = TypeIoWriter::new();
        writer.b(16);
        assert!(matches!(
            read_kick(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::UnknownTag { .. })
        ));

        for control in MarkerControl::ALL {
            assert_eq!(
                round_trip(
                    |w, value| write_marker_control(w, *value),
                    read_marker_control,
                    control
                ),
                control
            );
            assert_eq!(
                MarkerControl::from_ordinal(control.ordinal()),
                Some(control)
            );
            assert!(!control.name().is_empty());
        }
        assert_eq!(MarkerControl::from_ordinal(26), None);

        for action in AdminAction::ALL {
            assert_eq!(
                round_trip(|w, value| write_action(w, *value), read_action, action),
                action
            );
            assert_eq!(AdminAction::from_ordinal(action.ordinal()), Some(action));
        }
        assert_eq!(AdminAction::from_ordinal(5), None);
    }

    #[test]
    fn sound_effect_block_team_and_color_round_trip() {
        assert_eq!(
            round_trip(
                |w, value| write_sound(w, *value),
                read_sound,
                Some(SoundId(12))
            ),
            Some(SoundId(12))
        );
        assert_eq!(
            round_trip(|w, value| write_sound(w, *value), read_sound, None),
            None
        );
        assert_eq!(
            round_trip(
                |w, value| write_effect(w, *value),
                read_effect,
                Some(EffectId(88))
            ),
            Some(EffectId(88))
        );
        assert_eq!(
            round_trip(
                |w, value| write_block(w, *value),
                read_block,
                Some(BlockId(7))
            ),
            Some(BlockId(7))
        );
        assert_eq!(
            round_trip(|w, value| write_team(w, Some(*value)), read_team, TeamId(4)),
            TeamId(4)
        );
        let color = Rgba {
            r: 0x12,
            g: 0x34,
            b: 0x56,
            a: 0x78,
        };
        assert_eq!(
            round_trip(|w, value| write_color(w, *value), read_color, color),
            color
        );
        assert_eq!(color.to_rgba(), 0x1234_5678);
        assert_eq!(Rgba::from_rgba(color.to_rgba()), color);
        assert_eq!(color.to_string(), "#12345678");
    }

    #[test]
    fn trace_info_round_trips_with_caps() {
        let trace = TraceInfo {
            ip: Some("127.0.0.1".to_string()),
            uuid: Some("abcd".to_string()),
            locale: None,
            modded: true,
            mobile: false,
            times_joined: 12,
            times_kicked: 2,
            ips: vec![Some("1.2.3.4".to_string()), None],
            names: vec![Some("player".to_string())],
        };
        assert_eq!(
            round_trip(
                |w, value| write_trace_info(w, value).unwrap(),
                read_trace_info,
                trace.clone()
            ),
            trace
        );

        // The writer caps at 12 and the reader rejects more.
        let trace = TraceInfo {
            ips: (0..20).map(|i| Some(i.to_string())).collect(),
            ..TraceInfo::default()
        };
        let mut writer = TypeIoWriter::new();
        write_trace_info(&mut writer, &trace).unwrap();
        let mut reader = TypeIoReader::new(writer.as_bytes());
        assert_eq!(
            read_trace_info(&mut reader).unwrap().ips.len(),
            TRACE_LIST_LIMIT
        );

        // Hand-built 13-entry list: rejected on read.
        let mut writer = TypeIoWriter::new();
        write_string(&mut writer, None).unwrap();
        write_string(&mut writer, None).unwrap();
        write_string(&mut writer, None).unwrap();
        writer.b(0);
        writer.b(0);
        writer.i(0);
        writer.i(0);
        writer.ub(13);
        for _ in 0..13 {
            write_string(&mut writer, None).unwrap();
        }
        writer.ub(0);
        assert!(matches!(
            read_trace_info(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::TooLarge { .. })
        ));
    }

    #[test]
    fn string_array_round_trips_and_rejects_cells() {
        let options = vec![
            vec![Some("Resume".to_string()), Some("Restart".to_string())],
            vec![None],
            vec![],
        ];
        assert_eq!(
            round_trip(
                |w, value| write_string_array(w, value).unwrap(),
                read_string_array,
                options.clone()
            ),
            options
        );

        // 255 rows x 255 columns exceeds the cell cap.
        let mut writer = TypeIoWriter::new();
        writer.ub(255);
        for _ in 0..255 {
            writer.ub(255);
            for _ in 0..255 {
                write_string(&mut writer, None).unwrap();
            }
        }
        assert!(matches!(
            read_string_array(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::TooLarge { .. })
        ));
    }

    #[test]
    fn menu_results_round_trip_all_value_kinds_and_bounds() {
        let result = MenuResult {
            token: 0x1122_3344_5566_7788,
            result: Some("ok".to_string()),
            values: vec![
                ("slider".to_string(), MenuValue::Float(0.75)),
                ("check".to_string(), MenuValue::Bool(true)),
                ("field".to_string(), MenuValue::Str("text".to_string())),
            ],
        };
        assert_eq!(
            round_trip(
                |w, value| write_menu_result(w, value).unwrap(),
                read_menu_result,
                result.clone()
            ),
            result
        );
        assert!(!result.was_cancelled());
        assert!(result.result_is("ok"));
        assert_eq!(result.value("slider"), Some(&MenuValue::Float(0.75)));
        assert!(MenuResult::default().was_cancelled());

        // Too many entries.
        let mut writer = TypeIoWriter::new();
        writer.l(0);
        write_string(&mut writer, None).unwrap();
        writer.us((MENU_RESULT_MAX_TOTAL_VALUES + 1) as u16);
        assert!(matches!(
            read_menu_result(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::TooLarge { .. })
        ));

        // Unknown value tag.
        let mut writer = TypeIoWriter::new();
        writer.l(1);
        write_string(&mut writer, None).unwrap();
        writer.us(1);
        writer.str("key").unwrap();
        writer.b(7);
        assert!(matches!(
            read_menu_result(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::UnknownTag { .. })
        ));

        // Result string over the cap.
        let mut writer = TypeIoWriter::new();
        writer.l(1);
        write_string(
            &mut writer,
            Some(&"x".repeat(MENU_RESULT_MAX_RESULT_LEN + 1)),
        )
        .unwrap();
        writer.us(0);
        assert!(matches!(
            read_menu_result(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::TooLarge { .. })
        ));

        // Total-string budget exhaustion: six max-length values overflow the
        // 6000-char budget on the last value.
        let value = "y".repeat(MENU_RESULT_MAX_VALUE_LEN);
        let mut writer = TypeIoWriter::new();
        writer.l(1);
        write_string(&mut writer, None).unwrap();
        writer.us(6);
        for index in 0..6 {
            writer.str(&format!("k{index}")).unwrap();
            writer.b(2);
            writer.str(&value).unwrap();
        }
        assert!(matches!(
            read_menu_result(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::TooLarge { .. })
        ));
    }

    #[test]
    fn ui_builder_round_trips_nested_nodes_and_rejects_malformed() {
        let node = UiNode::new(UiKey::Table)
            .with(UiKey::Id, UiValue::Str("root".to_string()))
            .with(UiKey::Margin, UiValue::Float(4.0))
            .with(UiKey::Row, UiValue::Bool(true))
            .with(
                UiKey::Label,
                UiValue::Node(Box::new(
                    UiNode::new(UiKey::Label)
                        .with(UiKey::Text, UiValue::Str("hello".to_string()))
                        .with(UiKey::Wrap, UiValue::Bool(true)),
                )),
            );
        let decoded = round_trip(
            |w, value| write_ui_builder(w, value).unwrap(),
            read_ui_builder,
            node.clone(),
        );
        assert_eq!(decoded, node);
        assert_eq!(decoded.str_value(UiKey::Id), Some("root"));
        assert_eq!(decoded.num_value(UiKey::Margin), Some(4.0));
        assert_eq!(decoded.bool_value(UiKey::Row), Some(true));
        assert_eq!(UiKey::ALL.len(), 64);
        assert_eq!(UiKey::NODE_TYPES.len(), 13);
        assert!(!UiKey::Row.is_node_type());
        for (index, key) in UiKey::ALL.iter().enumerate() {
            assert_eq!(key.ordinal(), index as u8);
            assert_eq!(UiKey::from_ordinal(key.ordinal()), Some(*key));
            assert!(!key.name().is_empty());
        }

        // A non-node key at a node position is malformed.
        let mut writer = TypeIoWriter::new();
        writer.ub(UiKey::Text.ordinal());
        writer.s(0);
        assert!(matches!(
            read_ui_builder(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::InvalidValue { .. })
        ));

        // Unknown entry key / value tag are rejected.
        let mut writer = TypeIoWriter::new();
        writer.ub(UiKey::Table.ordinal());
        writer.s(1);
        writer.s(9_999);
        writer.b(0);
        writer.str("x").unwrap();
        assert!(matches!(
            read_ui_builder(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::UnknownTag {
                what: "ui entry key",
                ..
            })
        ));

        let mut writer = TypeIoWriter::new();
        writer.ub(UiKey::Table.ordinal());
        writer.s(1);
        writer.s(i16::from(UiKey::Text.ordinal()));
        writer.b(9);
        assert!(matches!(
            read_ui_builder(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::UnknownTag {
                what: "ui entry value",
                ..
            })
        ));

        // Depth cap: 20 nested children are rejected.
        let mut writer = TypeIoWriter::new();
        for _ in 0..=MAX_UI_DEPTH + 1 {
            writer.ub(UiKey::Table.ordinal());
            writer.s(1);
            writer.s(i16::from(UiKey::Table.ordinal()));
            writer.b(3);
        }
        writer.ub(UiKey::Table.ordinal());
        writer.s(0);
        assert!(matches!(
            read_ui_builder(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::DepthExceeded { .. })
        ));

        // Entry-count cap.
        let mut writer = TypeIoWriter::new();
        writer.ub(UiKey::Table.ordinal());
        writer.us((MAX_UI_ENTRIES + 1) as u16);
        assert!(matches!(
            read_ui_builder(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::TooLarge { .. })
        ));
    }

    #[test]
    fn point_packing_matches_point2() {
        assert_eq!(pack_point2(3, 4), (3 << 16) | 4);
        assert_eq!(unpack_point2_x(pack_point2(3, 4)), 3);
        assert_eq!(unpack_point2_y(pack_point2(3, 4)), 4);
        assert_eq!(pack_point2(-1, -1), NULL_TILE_POS);
    }
}

/// PAR-0578: the `writeObject` tagged codec plus the plan/mount/ability sync
/// bodies.
#[cfg(test)]
mod object_io_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn encode_object(value: &TaggedValue) -> Vec<u8> {
        let mut writer = TypeIoWriter::new();
        write_object(&mut writer, value).unwrap();
        writer.into_bytes()
    }

    fn decode_object(bytes: &[u8]) -> TaggedValue {
        let mut reader = TypeIoReader::new(bytes);
        let value = read_object(&mut reader).unwrap();
        reader.expect_end().unwrap();
        value
    }

    /// Every tag the Java `writeObject` can emit round-trips through the port;
    /// `legacyType` reads back as null like `readObject`'s `yield null`.
    #[test]
    fn tagged_objects_round_trip_every_wire_tag() {
        let values = vec![
            TaggedValue::Null,
            TaggedValue::Int(-5),
            TaggedValue::Long(1 << 40),
            TaggedValue::Float(1.5),
            TaggedValue::String(Some("hello".to_string())),
            TaggedValue::String(None),
            TaggedValue::Content {
                content_type: 2,
                id: 7,
            },
            TaggedValue::IntSeq(vec![1, -2, 3]),
            TaggedValue::Point2 { x: -3, y: 9 },
            TaggedValue::Point2Array(vec![(1, 2), (-3, 4)]),
            TaggedValue::TechNode {
                content_type: 1,
                id: 3,
            },
            TaggedValue::Boolean(true),
            TaggedValue::Double(2.5),
            TaggedValue::Building(1234),
            TaggedValue::LAccess(5),
            TaggedValue::Bytes(vec![1, 2, 3]),
            TaggedValue::Bools(vec![true, false, true]),
            TaggedValue::Unit(42),
            TaggedValue::Vec2Array(vec![(1.0, 2.0), (-3.5, 4.5)]),
            TaggedValue::Vec2 { x: 1.5, y: -2.5 },
            TaggedValue::Team(3),
            TaggedValue::IntArray(vec![7, -8]),
            TaggedValue::ObjectArray(vec![
                TaggedValue::Int(1),
                TaggedValue::String(Some("x".to_string())),
                TaggedValue::Null,
            ]),
            TaggedValue::UnitCommand(9),
        ];
        for value in values {
            assert_eq!(decode_object(&encode_object(&value)), value, "{value:?}");
        }

        // `legacyType` (tag 15) skips one byte and reads as null upstream.
        assert_eq!(decode_object(&[15, 0xff]), TaggedValue::Null);
        // `Bullet`/`Seq` were written as tag 0 upstream.
        assert_eq!(decode_object(&[0]), TaggedValue::Null);
    }

    /// Array/byte/string caps mirror `TypeIO.maxArraySize`/`maxByteArraySize`
    /// and `readObjectSafe`'s 1200-char string cap; nested arrays (Java's
    /// `allowArrays = false` recursion) are rejected.
    #[test]
    fn tagged_object_caps_and_nesting_are_enforced() {
        // Byte array over `maxByteArraySize` cannot be written...
        let huge = TaggedValue::Bytes(vec![0; MAX_TAGGED_BYTES + 1]);
        let mut writer = TypeIoWriter::new();
        assert!(matches!(
            write_object(&mut writer, &huge),
            Err(TypeIoError::TooLarge { .. })
        ));

        // ...nor read: tag 14 + i(MAX+1).
        let mut writer = TypeIoWriter::new();
        writer.b(14);
        writer.i((MAX_TAGGED_BYTES + 1) as i32);
        assert!(matches!(
            read_object(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::TooLarge { .. })
        ));

        // An int seq over `maxArraySize` is rejected on write and read.
        let huge_seq = TaggedValue::IntSeq(vec![0; MAX_TAGGED_ARRAY + 1]);
        let mut writer = TypeIoWriter::new();
        assert!(matches!(
            write_object(&mut writer, &huge_seq),
            Err(TypeIoError::TooLarge { .. })
        ));
        let mut writer = TypeIoWriter::new();
        writer.b(6);
        writer.s((MAX_TAGGED_ARRAY + 1) as i16);
        assert!(matches!(
            read_object(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::TooLarge { .. })
        ));

        // Nested arrays: an int seq inside an object array is `allowArrays =
        // false` upstream, so it is a rejection (never a panic).
        let mut writer = TypeIoWriter::new();
        writer.b(22);
        writer.i(1);
        writer.b(6);
        writer.s(1);
        writer.i(7);
        assert!(matches!(
            read_object(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::InvalidValue {
                what: "nested tagged array",
                ..
            })
        ));

        // An unknown tag is a rejection.
        assert!(matches!(
            read_object(&mut TypeIoReader::new(&[24])),
            Err(TypeIoError::UnknownTag { tag: 24, .. })
        ));
    }

    /// The network plan queue round-trips both plan shapes and is capped at
    /// `maxSyncedPlans`/the config budget on write and read.
    #[test]
    fn plan_queue_round_trips_and_caps() {
        let plans = vec![
            BuildPlan {
                breaking: false,
                x: 3,
                y: 4,
                block: Some(BlockId(11)),
                rotation: 2,
                config: TaggedValue::Content {
                    content_type: 2,
                    id: 7,
                },
            },
            BuildPlan::breaking(-3, -4),
        ];
        let mut writer = TypeIoWriter::new();
        write_plans_queue_net(&mut writer, Some(&plans)).unwrap();
        let mut reader = TypeIoReader::new(writer.as_bytes());
        assert_eq!(read_plans_queue_net(&mut reader).unwrap(), Some(plans));

        // Null plan list.
        let mut writer = TypeIoWriter::new();
        write_plans_queue_net(&mut writer, None).unwrap();
        let mut reader = TypeIoReader::new(writer.as_bytes());
        assert_eq!(read_plans_queue_net(&mut reader).unwrap(), None);

        // The `short`-count variant (`writePlans`/`readPlans`).
        let plans = vec![BuildPlan::breaking(1, 2)];
        let mut writer = TypeIoWriter::new();
        write_plans(&mut writer, Some(&plans)).unwrap();
        let mut reader = TypeIoReader::new(writer.as_bytes());
        assert_eq!(read_plans(&mut reader).unwrap(), Some(plans));
        let mut writer = TypeIoWriter::new();
        write_plans(&mut writer, None).unwrap();
        assert_eq!(
            read_plans(&mut TypeIoReader::new(writer.as_bytes())).unwrap(),
            None
        );

        // Over-long queues are rejected on read, not truncated silently.
        let mut writer = TypeIoWriter::new();
        writer.i((MAX_SYNCED_PLANS + 1) as i32);
        assert!(matches!(
            read_plans_queue_net(&mut TypeIoReader::new(writer.as_bytes())),
            Err(TypeIoError::TooLarge { .. })
        ));

        // `getMaxPlans` budget: a 400-byte config then a 200-byte one crosses
        // the 500-byte budget at plan 2.
        let budget = vec![
            BuildPlan {
                config: TaggedValue::Bytes(vec![0; 400]),
                ..BuildPlan::breaking(0, 0)
            },
            BuildPlan {
                config: TaggedValue::Bytes(vec![0; 200]),
                ..BuildPlan::breaking(0, 1)
            },
        ];
        assert_eq!(max_synced_plans(&budget), 2);
        let many = vec![BuildPlan::breaking(0, 0); MAX_SYNCED_PLANS + 5];
        assert_eq!(max_synced_plans(&many), MAX_SYNCED_PLANS);
    }

    /// Client preview plans write the rotation only for rotating blocks and
    /// drop non-preview configs (Java `validClientPlanConfig`).
    #[test]
    fn client_plans_round_trip_with_rotation_content() {
        let plans = vec![
            ClientPlan {
                x: 5,
                y: 6,
                block: BlockId(3),
                rotates: true,
                rotation: 2,
                config: TaggedValue::Int(5),
            },
            ClientPlan {
                x: 7,
                y: 8,
                block: BlockId(4),
                rotates: false,
                rotation: 0,
                config: TaggedValue::ObjectArray(vec![TaggedValue::Int(1)]),
            },
        ];
        let mut writer = TypeIoWriter::new();
        write_client_plans(&mut writer, Some(&plans)).unwrap();
        let mut reader = TypeIoReader::new(writer.as_bytes());
        let read = read_client_plans(&mut reader, |block| block == 3)
            .unwrap()
            .expect("plans");
        assert_eq!(read.len(), 2);
        assert_eq!(read[0], plans[0]);
        // A link config is not a valid preview config: it goes out as null.
        assert_eq!(read[1].config, TaggedValue::Null);
        assert_eq!(read[1].block, BlockId(4));
        assert_eq!(read[1].rotation, 0);

        // Empty/null writes read back as `None`.
        let mut writer = TypeIoWriter::new();
        write_client_plans(&mut writer, None).unwrap();
        assert_eq!(
            read_client_plans(&mut TypeIoReader::new(writer.as_bytes()), |_| true).unwrap(),
            None
        );

        // The reduced config tag set rejects an array tag.
        let mut writer = TypeIoWriter::new();
        writer.s(1);
        writer.i(pack_point2(1, 1));
        writer.us(3);
        writer.b(0); // rotation for the rotating block 3
        writer.b(6); // int seq: not a valid client plan config
        assert!(matches!(
            read_client_plans(&mut TypeIoReader::new(writer.as_bytes()), |_| true),
            Err(TypeIoError::UnknownTag { tag: 6, .. })
        ));
    }

    /// `writeMounts`/`writeAbilities` bodies round-trip; the state byte packs
    /// shoot/rotate.
    #[test]
    fn mounts_and_abilities_round_trip() {
        let mounts = vec![
            WeaponMount {
                shoot: true,
                rotate: false,
                aim_x: 1.5,
                aim_y: -2.5,
            },
            WeaponMount {
                shoot: false,
                rotate: true,
                aim_x: 0.0,
                aim_y: 3.0,
            },
            WeaponMount::default(),
        ];
        let mut writer = TypeIoWriter::new();
        write_mounts(&mut writer, &mounts).unwrap();
        let mut reader = TypeIoReader::new(writer.as_bytes());
        assert_eq!(read_mounts(&mut reader).unwrap(), mounts);

        let abilities = vec![0.0, 1.5, -2.0];
        let mut writer = TypeIoWriter::new();
        write_abilities(&mut writer, &abilities).unwrap();
        let mut reader = TypeIoReader::new(writer.as_bytes());
        assert_eq!(read_abilities(&mut reader).unwrap(), abilities);

        // Over-long mount/ability lists are rejected on write.
        let mut writer = TypeIoWriter::new();
        assert!(matches!(
            write_mounts(&mut writer, &vec![WeaponMount::default(); 256]),
            Err(TypeIoError::TooLarge { .. })
        ));
        let mut writer = TypeIoWriter::new();
        assert!(matches!(
            write_abilities(&mut writer, &vec![0.0; 256]),
            Err(TypeIoError::TooLarge { .. })
        ));
    }

    /// `KickReason.from_name` (the producer token parser).
    #[test]
    fn kick_reason_names_parse_back() {
        assert_eq!(
            KickReason::from_name("gameover"),
            Some(KickReason::Gameover)
        );
        assert_eq!(
            KickReason::from_name("ServerClose"),
            Some(KickReason::ServerClose)
        );
        assert_eq!(KickReason::from_name(" vote "), Some(KickReason::Vote));
        assert_eq!(KickReason::from_name("nope"), None);
        for reason in KickReason::ALL {
            assert_eq!(KickReason::from_name(reason.name()), Some(reason));
        }
    }
}
