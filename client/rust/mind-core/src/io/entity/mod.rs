// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Entity IO & revisions (plan 04 §3.5).
//!
//! Rust equivalent of the generated upstream entity read/write chain
//! (`annotations/.../entity/EntityIO.java` + `revisions/<def>/<N>.json`):
//! a derive macro ([`mind_derive::EntityIo`]) with explicit flags, committed
//! revision manifests checked by `mind-headless io check-revisions`, an
//! append-only class-ID registry (`entity_class_ids.toml` → `class_ids.rs`),
//! and the entity ID mapping used by the `entities` region.
//!
//! Ported from `annotations/src/main/java/mindustry/annotations/entity/EntityIO.java`
//! and `annotations/src/main/resources/{revisions/**,classids.properties}`.

pub mod class_ids;
pub mod field;
pub mod idfile;
pub mod idmap;
pub mod registry;
pub mod revisions;
#[cfg(test)]
mod tests;

pub use field::IoField;
pub use idmap::{DuplicateIdTracker, EntityIdMap};

use super::IoResult;
use super::wire::{WireReader, WireWriter};

/// Entity write stream (the Arc `Writes` the generated code used).
pub type EntityWriter<'a> = WireWriter<'a>;

/// Entity read stream (the Arc `Reads` the generated code used).
pub type EntityReader<'a> = WireReader<'a>;

/// One serialized field descriptor (`revisions/<def>/<N>.json` entry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldDesc {
    /// Field name (Rust declaration name).
    pub name: &'static str,
    /// Wire type name (`IoField::TYPE_NAME`).
    pub type_: &'static str,
    /// Byte size for fixed-size types, `-1` for variable/complex.
    pub size: i32,
    /// Field flags (manifest strings per plan 04 §6.4).
    pub flags: &'static [FieldFlag],
}

/// Field flags recorded in revision manifests (§6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFlag {
    /// Serialized in saves.
    Save,
    /// Serialized in network sync.
    Sync,
    /// `@SyncLocal` — only synced to the local client.
    SyncLocal,
    /// `@SyncField` with linear interpolation.
    InterpLinear,
    /// `@SyncField` with angle interpolation.
    InterpAngle,
    /// Interpolated value is clamped.
    Clamped,
    /// Neither saved nor synced (recomputed).
    Transient,
}

impl FieldFlag {
    /// Manifest string form (§6.4).
    pub const fn as_str(self) -> &'static str {
        match self {
            FieldFlag::Save => "save",
            FieldFlag::Sync => "sync",
            FieldFlag::SyncLocal => "sync_local",
            FieldFlag::InterpLinear => "interp:linear",
            FieldFlag::InterpAngle => "interp:angle",
            FieldFlag::Clamped => "clamped",
            FieldFlag::Transient => "transient",
        }
    }

    /// Parses the manifest string form.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "save" => Some(FieldFlag::Save),
            "sync" => Some(FieldFlag::Sync),
            "sync_local" => Some(FieldFlag::SyncLocal),
            "interp:linear" => Some(FieldFlag::InterpLinear),
            "interp:angle" => Some(FieldFlag::InterpAngle),
            "clamped" => Some(FieldFlag::Clamped),
            "transient" => Some(FieldFlag::Transient),
            _ => None,
        }
    }
}

/// Interpolation mode exported for plans 16/21 (`@SyncField` metadata, R11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interp {
    /// `Mathf.lerp`.
    Linear,
    /// `Mathf.slerp` (angles).
    Angle,
    /// No interpolation.
    None,
}

/// `@SyncField` metadata (pure data export; storage/runtime is plan 05/16/21).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncFieldMeta {
    /// Field name.
    pub name: &'static str,
    /// Interpolation mode.
    pub interp: Interp,
    /// Whether the interpolated value is clamped.
    pub clamped: bool,
}

/// The entity codec contract (plan 04 §3.5), implemented by
/// `#[derive(EntityIo)]`.
pub trait EntityCodec: Sized {
    /// Element/def name (`BuildingComp`), matching `revisions/<NAME>/`.
    const NAME: &'static str;
    /// Network/save class ID from `entity_class_ids.toml` (append-only).
    const CLASS_ID: u8;
    /// Whether the def serializes in saves (`@EntityDef(serialize)`).
    const SERIALIZE: bool;
    /// Whether the def serializes in sync (`genio`).
    const SYNC: bool;
    /// Newest revision number written by `write`/`write_sync`.
    const NEWEST_REVISION: u16;

    /// Newest-revision save field list (declaration order).
    fn fields() -> &'static [FieldDesc];
    /// `@SyncField` metadata export.
    fn sync_fields() -> &'static [SyncFieldMeta];
    /// Writes the newest revision (`u16`) then save fields.
    fn write(&self, w: &mut EntityWriter) -> IoResult<()>;
    /// Reads fields for one revision; unknown revisions error.
    fn read(&mut self, r: &mut EntityReader, revision: u16) -> IoResult<()>;
    /// Writes the newest revision (`u16`) then sync fields.
    fn write_sync(&self, w: &mut EntityWriter) -> IoResult<()>;
    /// Reads sync fields for one revision.
    fn read_sync(&mut self, r: &mut EntityReader, revision: u16) -> IoResult<()>;

    /// Building IO format byte written at the start of a tile-entity chunk
    /// (`entity.version()`; `BuildingComp.version()` returns 0 upstream).
    const TILE_VERSION: u8 = 0;

    /// Interpolation hook for plans 16/21 (`EntityIO.interpolate`).
    ///
    /// Plan 04 exports metadata only (R11); the default is a no-op until the
    /// interpolation runtime lands.
    fn interpolate(&self, _target: &Self, _alpha: f32) {}
}

/// Static metadata for one registered entity def (`EntityDefs!` rows).
#[derive(Debug, Clone, Copy)]
pub struct EntityDefMeta {
    /// Def name.
    pub name: &'static str,
    /// Class ID (`entity_class_ids.toml`).
    pub class_id: u8,
    /// Serialized in saves.
    pub serialize: bool,
    /// Serialized in sync.
    pub sync: bool,
    /// Save fields (newest revision).
    pub fields: &'static [FieldDesc],
    /// Sync field metadata.
    pub sync_fields: &'static [SyncFieldMeta],
}
