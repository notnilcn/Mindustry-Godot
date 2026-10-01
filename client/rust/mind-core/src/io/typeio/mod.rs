// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TypeIO` equivalent: the byte-tagged object codec (plan 04 §3.4/§6.3).
//!
//! Ported from `core/src/mindustry/io/TypeIO.java` (`writeObject`,
//! `readObjectBoxed`, `readObject`, `readObjectSafe`). Used by tile configs,
//! build plans, logic vars, `@Remote` parameters and schematic configs.
//!
//! - Tags are frozen (`tags.rs`); unknown tags are an error, never a panic.
//! - Caps match upstream: [`MAX_ARRAY`], [`MAX_BYTE_ARRAY`],
//!   [`MAX_SYNCED_PLANS`], [`MAX_SAFE_STRING`]; non-safe object reads cap
//!   arrays at 200 (upstream quirk kept).
//! - `read_object_safe` is the only entry point for untrusted network input
//!   (plan 21) and is always used for plan configs.
//! - All functions take `&mut` reader/writer state and hold no shared mutable
//!   state, satisfying upstream's thread-safety note by construction.

pub mod codecs;
pub mod mapper;
pub mod tags;

use smallvec::SmallVec;

use super::wire::{WireReader, WireWriter};
use super::{IoError, IoResult};
use crate::content::ContentType;
use mapper::ContentMapper;

/// `TypeIO.maxArraySize` — generic array cap.
pub const MAX_ARRAY: usize = 1000;
/// `TypeIO.maxByteArraySize` — byte-array cap.
pub const MAX_BYTE_ARRAY: usize = 40_000;
/// `TypeIO.maxSyncedPlans` — network plan-queue cap.
pub const MAX_SYNCED_PLANS: usize = 20;
/// Safe string cap (`Reads.str(1200)` in `readObjectSafe`).
pub const MAX_SAFE_STRING: usize = 1200;
/// Upstream quirk: *non-safe* object reads cap arrays at 200
/// (`readObject`: `maxArraySize = safe ? TypeIO.maxArraySize : 200`).
pub const MAX_UNSAFE_ARRAY: usize = 200;
/// Rules JSON byte cap (`TypeIO.readRules`).
pub const MAX_RULES_BYTES: usize = 100_000;
/// Objectives JSON byte cap (`TypeIO.readObjectives`).
pub const MAX_OBJECTIVES_BYTES: usize = 60_000;
/// `Maps.maxPlayerPreviewPlans` — client plan preview cap.
pub const MAX_PLAYER_PREVIEW_PLANS: usize = 1000;

/// Point packing (`Point2.pack`): `(x << 16) | (y & 0xFFFF)`, both
/// short-truncated on unpack.
pub fn pack_point2(x: i32, y: i32) -> i32 {
    (x << 16) | (y & 0xFFFF)
}

/// `Point2.x(pack)`.
pub fn unpack_point2_x(pack: i32) -> i32 {
    i32::from((pack >> 16) as i16)
}

/// `Point2.y(pack)`.
pub fn unpack_point2_y(pack: i32) -> i32 {
    i32::from(pack as i16)
}

/// Entity reference carried by `Building`/`Unit` values.
///
/// Upstream resolves against the world immediately (`world.build`,
/// `Groups.unit.getByID`) or boxes the raw id (`BuildingBox`/`UnitBox`). The
/// native codec keeps the raw id; plan 05's `EntityIndex` resolves it, and
/// refs read before their target lands become `Pending` + a
/// [`super::save::SaveReadState::pending_entity_refs`] entry resolved by the
/// `after_read_all` pass (plan 04 §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityRef {
    /// Raw entity id / packed tile pos as read or written.
    Id(i32),
    /// Read before the target entity was loaded; resolved by `after_read_all`.
    Pending(i32),
}

impl EntityRef {
    /// The raw id.
    pub const fn raw(self) -> i32 {
        match self {
            EntityRef::Id(id) | EntityRef::Pending(id) => id,
        }
    }
}

/// One tagged object value (`writeObject`/`readObject` payload).
#[derive(Debug, Clone, PartialEq)]
pub enum TypeValue {
    /// Tag 0 — absent value (also produced for bullets/`Seq`, upstream).
    Null,
    /// Tag 1.
    Int(i32),
    /// Tag 2.
    Long(i64),
    /// Tag 3.
    Float(f32),
    /// Tag 4 — string with existence byte (`None` = Java null).
    Str(Option<String>),
    /// Tag 5 — content type ordinal + raw id (resolution is the caller's job).
    Content(ContentType, u16),
    /// Tag 6.
    IntSeq(SmallVec<[i32; 8]>),
    /// Tag 7.
    Point2(i32, i32),
    /// Tag 8 — packed points.
    Point2Array(SmallVec<[i32; 8]>),
    /// Tag 9 — content type ordinal + raw id of the node's content.
    TechNode(ContentType, u16),
    /// Tag 10.
    Bool(bool),
    /// Tag 11.
    Double(f64),
    /// Tag 12 — packed tile pos.
    Building(EntityRef),
    /// Tag 13 — `LAccess` ordinal.
    LAccess(u16),
    /// Tag 14.
    ByteArray(Vec<u8>),
    /// Tag 16.
    BoolArray(Vec<bool>),
    /// Tag 17 — entity id.
    Unit(EntityRef),
    /// Tag 18.
    Vec2Array(SmallVec<[(f32, f32); 8]>),
    /// Tag 19.
    Vec2(f32, f32),
    /// Tag 20 — team id.
    Team(u8),
    /// Tag 21.
    IntArray(Vec<i32>),
    /// Tag 22.
    ObjectArray(Vec<TypeValue>),
    /// Tag 23 — unit command content id.
    UnitCommand(u16),
}

fn too_large(actual: usize) -> IoError {
    // Upstream message: "Array size too large: N".
    IoError::corrupt(format!("Array size too large: {actual}"))
}

fn invalid_size(actual: i64) -> IoError {
    // Upstream message: "Invalid array size: N".
    IoError::corrupt(format!("Invalid array size: {actual}"))
}

/// Writes a tagged object (`TypeIO.writeObject`).
pub fn write_object(w: &mut WireWriter, value: &TypeValue) -> IoResult<()> {
    match value {
        TypeValue::Null => w.ub(tags::NULL),
        TypeValue::Int(v) => {
            w.ub(tags::INTEGER);
            w.i(*v);
        }
        TypeValue::Long(v) => {
            w.ub(tags::LONG);
            w.l(*v);
        }
        TypeValue::Float(v) => {
            w.ub(tags::FLOAT);
            w.f(*v);
        }
        TypeValue::Str(v) => {
            w.ub(tags::STRING);
            codecs::write_string(w, v.as_deref())?;
        }
        TypeValue::Content(type_, id) => {
            w.ub(tags::CONTENT);
            w.ub(type_.ordinal() as u8);
            w.s(*id as i16);
        }
        TypeValue::IntSeq(values) => {
            if values.len() > MAX_ARRAY {
                return Err(too_large(values.len()));
            }
            w.ub(tags::INT_SEQ);
            w.s(values.len() as i16);
            for v in values {
                w.i(*v);
            }
        }
        TypeValue::Point2(x, y) => {
            w.ub(tags::POINT2);
            w.i(*x);
            w.i(*y);
        }
        TypeValue::Point2Array(points) => {
            // Upstream: 255 is the limit (byte count field).
            if points.len() > 255 {
                return Err(too_large(points.len()));
            }
            w.ub(tags::POINT2_ARRAY);
            w.ub(points.len() as u8);
            for p in points {
                w.i(*p);
            }
        }
        TypeValue::TechNode(type_, id) => {
            w.ub(tags::TECH_NODE);
            w.ub(type_.ordinal() as u8);
            w.s(*id as i16);
        }
        TypeValue::Bool(v) => {
            w.ub(tags::BOOLEAN);
            w.bool(*v);
        }
        TypeValue::Double(v) => {
            w.ub(tags::DOUBLE);
            w.d(*v);
        }
        TypeValue::Building(pos) => {
            w.ub(tags::BUILDING);
            w.i(pos.raw());
        }
        TypeValue::LAccess(ordinal) => {
            w.ub(tags::L_ACCESS);
            w.s(*ordinal as i16);
        }
        TypeValue::ByteArray(bytes) => {
            if bytes.len() > MAX_BYTE_ARRAY {
                return Err(too_large(bytes.len()));
            }
            w.ub(tags::BYTE_ARRAY);
            w.i(bytes.len() as i32);
            w.bytes(bytes);
        }
        TypeValue::BoolArray(values) => {
            if values.len() > MAX_ARRAY {
                return Err(too_large(values.len()));
            }
            w.ub(tags::BOOL_ARRAY);
            w.i(values.len() as i32);
            for v in values {
                w.bool(*v);
            }
        }
        TypeValue::Unit(id) => {
            w.ub(tags::UNIT);
            w.i(id.raw());
        }
        TypeValue::Vec2Array(vecs) => {
            if vecs.len() > MAX_ARRAY {
                return Err(too_large(vecs.len()));
            }
            w.ub(tags::VEC2_ARRAY);
            w.s(vecs.len() as i16);
            for (x, y) in vecs {
                w.f(*x);
                w.f(*y);
            }
        }
        TypeValue::Vec2(x, y) => {
            w.ub(tags::VEC2);
            w.f(*x);
            w.f(*y);
        }
        TypeValue::Team(id) => {
            w.ub(tags::TEAM);
            w.ub(*id);
        }
        TypeValue::IntArray(values) => {
            if values.len() > MAX_ARRAY {
                return Err(too_large(values.len()));
            }
            w.ub(tags::INT_ARRAY);
            codecs::write_ints(w, values)?;
        }
        TypeValue::ObjectArray(values) => {
            if values.len() > MAX_ARRAY {
                return Err(too_large(values.len()));
            }
            w.ub(tags::OBJECT_ARRAY);
            w.i(values.len() as i32);
            for value in values {
                write_object(w, value)?;
            }
        }
        TypeValue::UnitCommand(id) => {
            w.ub(tags::UNIT_COMMAND);
            w.s(*id as i16);
        }
    }
    Ok(())
}

/// `TypeIO.readObjectBoxed(read, false)`: unsafe defaults, arrays allowed.
pub fn read_object(r: &mut WireReader) -> IoResult<TypeValue> {
    read_object_with(r, false, None, false, true)
}

/// `TypeIO.readObjectSafe`: the untrusted-input entry point (plan 21, plan
/// configs).
pub fn read_object_safe(r: &mut WireReader) -> IoResult<TypeValue> {
    read_object_with(r, false, None, true, true)
}

/// Full read (`TypeIO.readObject(read, box, mapper, safe, allowArrays)`).
///
/// `boxed` keeps entity references unresolved (upstream `BuildingBox`/`UnitBox`;
/// the native codec always carries raw ids, so this only documents intent).
/// `mapper` remaps content ids through a save's temporary mapper.
pub fn read_object_with(
    r: &mut WireReader,
    boxed: bool,
    mapper: Option<&dyn ContentMapper>,
    safe: bool,
    allow_arrays: bool,
) -> IoResult<TypeValue> {
    let tag = r.ub()?;
    read_object_tag(r, boxed, mapper, safe, allow_arrays, tag)
}

/// Reads one object of a known tag (`TypeIO.readObject(..., byte type)`).
pub fn read_object_tag(
    r: &mut WireReader,
    _boxed: bool,
    mapper: Option<&dyn ContentMapper>,
    safe: bool,
    allow_arrays: bool,
    tag: u8,
) -> IoResult<TypeValue> {
    // Upstream quirk: non-safe reads use a lower array cap (200).
    let max_array = if safe { MAX_ARRAY } else { MAX_UNSAFE_ARRAY };
    let nested_err = || IoError::corrupt("Nested arrays are not allowed");
    match tag {
        tags::NULL => Ok(TypeValue::Null),
        tags::INTEGER => Ok(TypeValue::Int(r.i()?)),
        tags::LONG => Ok(TypeValue::Long(r.l()?)),
        tags::FLOAT => Ok(TypeValue::Float(r.f()?)),
        tags::STRING => {
            let exists = r.ub()?;
            if exists != 0 {
                let cap = if safe { MAX_SAFE_STRING } else { 0 };
                Ok(TypeValue::Str(Some(if cap > 0 {
                    r.str_capped(cap)?
                } else {
                    r.str()?
                })))
            } else {
                Ok(TypeValue::Str(None))
            }
        }
        tags::CONTENT => {
            let type_ = read_content_type(r)?;
            let id = r.s()? as u16;
            Ok(match mapper.and_then(|m| m.map_id(type_, id)) {
                Some(mapped) => TypeValue::Content(type_, mapped),
                None if mapper.is_some() => TypeValue::Null,
                None => TypeValue::Content(type_, id),
            })
        }
        tags::INT_SEQ => {
            if !allow_arrays {
                return Err(nested_err());
            }
            let len = r.s()?;
            if i64::from(len) > max_array as i64 {
                return Err(invalid_size(i64::from(len)));
            }
            let len = len.max(0) as usize;
            let mut out = SmallVec::with_capacity(len);
            for _ in 0..len {
                out.push(r.i()?);
            }
            Ok(TypeValue::IntSeq(out))
        }
        tags::POINT2 => Ok(TypeValue::Point2(r.i()?, r.i()?)),
        tags::POINT2_ARRAY => {
            if !allow_arrays {
                return Err(nested_err());
            }
            let len = r.ub()? as usize;
            let mut out = SmallVec::with_capacity(len);
            for _ in 0..len {
                out.push(r.i()?);
            }
            Ok(TypeValue::Point2Array(out))
        }
        tags::TECH_NODE => {
            let type_ = read_content_type(r)?;
            let id = r.s()? as u16;
            Ok(match mapper.and_then(|m| m.map_id(type_, id)) {
                Some(mapped) => TypeValue::TechNode(type_, mapped),
                None if mapper.is_some() => TypeValue::Null,
                None => TypeValue::TechNode(type_, id),
            })
        }
        tags::BOOLEAN => Ok(TypeValue::Bool(r.bool()?)),
        tags::DOUBLE => Ok(TypeValue::Double(r.d()?)),
        tags::BUILDING => Ok(TypeValue::Building(EntityRef::Id(r.i()?))),
        tags::L_ACCESS => Ok(TypeValue::LAccess(r.s()? as u16)),
        tags::BYTE_ARRAY => {
            if !allow_arrays {
                return Err(nested_err());
            }
            let len = r.i()?;
            if len < 0 || len as usize > MAX_BYTE_ARRAY {
                return Err(invalid_size(i64::from(len)));
            }
            Ok(TypeValue::ByteArray(r.bytes(len as usize)?.to_vec()))
        }
        tags::LEGACY => {
            // Old unit command byte; read and discard.
            let _ = r.ub()?;
            Ok(TypeValue::Null)
        }
        tags::BOOL_ARRAY => {
            if !allow_arrays {
                return Err(nested_err());
            }
            let len = r.i()?;
            if len < 0 || len as usize > max_array {
                return Err(invalid_size(i64::from(len)));
            }
            let mut out = Vec::with_capacity(len as usize);
            for _ in 0..len {
                out.push(r.bool()?);
            }
            Ok(TypeValue::BoolArray(out))
        }
        tags::UNIT => Ok(TypeValue::Unit(EntityRef::Id(r.i()?))),
        tags::VEC2_ARRAY => {
            if !allow_arrays {
                return Err(nested_err());
            }
            let len = r.s()?;
            if i64::from(len) > max_array as i64 {
                return Err(invalid_size(i64::from(len)));
            }
            let len = len.max(0) as usize;
            let mut out = SmallVec::with_capacity(len);
            for _ in 0..len {
                out.push((r.f()?, r.f()?));
            }
            Ok(TypeValue::Vec2Array(out))
        }
        tags::VEC2 => Ok(TypeValue::Vec2(r.f()?, r.f()?)),
        tags::TEAM => Ok(TypeValue::Team(r.ub()?)),
        // Upstream quirk: the int-array tag has no cap check on read.
        tags::INT_ARRAY => Ok(TypeValue::IntArray(codecs::read_ints(r)?)),
        tags::OBJECT_ARRAY => {
            if !allow_arrays {
                return Err(nested_err());
            }
            let len = r.i()?;
            if len < 0 || len as usize > max_array {
                return Err(invalid_size(i64::from(len)));
            }
            let mut out = Vec::with_capacity(len as usize);
            for _ in 0..len {
                out.push(read_object_with(r, _boxed, mapper, safe, false)?);
            }
            Ok(TypeValue::ObjectArray(out))
        }
        tags::UNIT_COMMAND => Ok(TypeValue::UnitCommand(r.us()?)),
        other => Err(IoError::UnknownTypeTag(other)),
    }
}

fn read_content_type(r: &mut WireReader) -> IoResult<ContentType> {
    let ordinal = r.ub()? as usize;
    ContentType::ALL
        .get(ordinal)
        .copied()
        .ok_or_else(|| IoError::corrupt(format!("invalid content type ordinal: {ordinal}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(value: &TypeValue) -> TypeValue {
        let mut buf = Vec::new();
        {
            let mut w = WireWriter::new(&mut buf);
            write_object(&mut w, value).unwrap();
        }
        let mut r = WireReader::new(&buf);
        let out = read_object(&mut r).unwrap();
        assert_eq!(r.remaining(), 0);
        out
    }

    #[test]
    fn all_tags_roundtrip() {
        let values = vec![
            TypeValue::Null,
            TypeValue::Int(-42),
            TypeValue::Long(9_000_000_000),
            TypeValue::Float(1.25),
            TypeValue::Str(Some("héllo".to_owned())),
            TypeValue::Str(None),
            TypeValue::Content(ContentType::Block, 79),
            TypeValue::IntSeq(SmallVec::from_vec(vec![1, -2, 3])),
            TypeValue::Point2(-3, 4),
            TypeValue::Point2Array(SmallVec::from_vec(vec![
                pack_point2(1, 2),
                pack_point2(-5, 6),
            ])),
            TypeValue::TechNode(ContentType::Item, 3),
            TypeValue::Bool(true),
            TypeValue::Double(-2.5),
            TypeValue::Building(EntityRef::Id(pack_point2(12, 34))),
            TypeValue::LAccess(7),
            TypeValue::ByteArray(vec![9, 8, 7]),
            TypeValue::BoolArray(vec![true, false, true]),
            TypeValue::Unit(EntityRef::Id(1234)),
            TypeValue::Vec2Array(SmallVec::from_vec(vec![(1.0, 2.0), (-3.5, 4.5)])),
            TypeValue::Vec2(-1.0, 0.5),
            TypeValue::Team(2),
            TypeValue::IntArray(vec![5, 6, 7]),
            TypeValue::ObjectArray(vec![TypeValue::Int(1), TypeValue::Bool(false)]),
            TypeValue::UnitCommand(3),
        ];
        for value in values {
            assert_eq!(roundtrip(&value), value.clone());
        }
    }

    #[test]
    fn point2_packing_parity() {
        // Arc Point2.pack/unpack, incl. negative coords.
        let packed = pack_point2(-3, 7);
        assert_eq!(unpack_point2_x(packed), -3);
        assert_eq!(unpack_point2_y(packed), 7);
        assert_eq!(pack_point2(0, 0), 0);
    }

    #[test]
    fn unknown_tag_is_an_error() {
        let buf = [200u8];
        let mut r = WireReader::new(&buf);
        assert!(matches!(
            read_object(&mut r),
            Err(IoError::UnknownTypeTag(200))
        ));
    }

    #[test]
    fn oversize_arrays_are_rejected_on_read_and_write() {
        // Read: declared int_seq length above the cap.
        let mut buf = Vec::new();
        {
            let mut w = WireWriter::new(&mut buf);
            w.ub(tags::INT_SEQ);
            w.s(2000);
        }
        let mut r = WireReader::new(&buf);
        assert!(read_object_safe(&mut r).is_err());

        // Non-safe reads cap at 200 (upstream quirk).
        let mut buf = Vec::new();
        {
            let mut w = WireWriter::new(&mut buf);
            w.ub(tags::BOOL_ARRAY);
            w.i(500);
        }
        let mut r = WireReader::new(&buf);
        assert!(read_object(&mut r).is_err());

        // Write: oversize byte array.
        let mut buf = Vec::new();
        let mut w = WireWriter::new(&mut buf);
        assert!(
            write_object(&mut w, &TypeValue::ByteArray(vec![0u8; MAX_BYTE_ARRAY + 1])).is_err()
        );
    }

    #[test]
    fn nested_arrays_are_rejected() {
        // An object array containing an int seq must fail (nested).
        let value = TypeValue::ObjectArray(vec![TypeValue::IntSeq(SmallVec::from_vec(vec![1]))]);
        let mut buf = Vec::new();
        {
            let mut w = WireWriter::new(&mut buf);
            write_object(&mut w, &value).unwrap();
        }
        let mut r = WireReader::new(&buf);
        let error = read_object(&mut r).unwrap_err();
        assert!(error.to_string().contains("Nested arrays are not allowed"));
    }

    #[test]
    fn safe_string_cap() {
        // Safe reads cap strings at 1200 bytes; unsafe reads do not.
        let long = "x".repeat(1300);
        let mut buf = Vec::new();
        {
            let mut w = WireWriter::new(&mut buf);
            w.ub(tags::STRING);
            w.ub(1);
            w.str(&long).unwrap();
        }
        let mut r = WireReader::new(&buf);
        assert!(read_object_safe(&mut r).is_err());
        let mut r = WireReader::new(&buf);
        assert_eq!(
            read_object(&mut r).unwrap(),
            TypeValue::Str(Some(long.clone()))
        );
    }

    #[test]
    fn legacy_tag_reads_null() {
        let mut buf = Vec::new();
        {
            let mut w = WireWriter::new(&mut buf);
            w.ub(tags::LEGACY);
            w.ub(5);
        }
        let mut r = WireReader::new(&buf);
        assert_eq!(read_object(&mut r).unwrap(), TypeValue::Null);
    }
}
