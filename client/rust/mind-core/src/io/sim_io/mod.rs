// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-sim save/load executor for the plan-05 `IoSet` boundary (plan 04 §3.10).
//!
//! The plan-04 `MindIo` gdext autoload delegates to [`SimIoHandler`]: requests
//! queued through [`crate::sim::Sim::request_save`]/`request_load` are fulfilled
//! at the `IoSet::Capture`/`IoSet::Apply` boundary against the live world. All
//! seven regions are written (`SaveVersion.java:80-88`): real meta tags
//! (`:114-151`), the tile `map` through plan-06's [`EcsMapSource`], the live
//! units + team build plans (`:457-465`) and the session's map markers.
//!
//! The load side applies the decoded buildings to the live runtime (materialize
//! queue), respawns the saved units and restores rules/wave/stats/markers/plans
//! onto the installed [`CampaignRuntime`] session — the `SaveSlot.load` half
//! (`Saves.java:314-328`).

use bevy_ecs::prelude::Resource;

use crate::sim::Sim;

mod apply;
mod decode;
mod entities;
mod handler;
#[cfg(test)]
mod map_filter_tests;
#[cfg(test)]
mod tests;
mod unit_codec;

pub(crate) use apply::apply_unit_record;
pub use decode::{DecodedSave, decode_map_bytes, decode_save_bytes};
pub use handler::{SectorResumeMeta, SimIoHandler, meta_tick};
pub use unit_codec::apply_team_plans;
pub(crate) use unit_codec::{read_unit_fields, write_unit_fields};

/// Reserved class id for the native live-sim unit chunk.
///
/// Upstream keys `writeWorldEntities` chunks by the unit type's generated class
/// id (`classids.properties`); native v1 stores the content name inside the
/// chunk instead, so one append-only id covers every unit type. Kept clear of
/// the upstream table (max id 49 at this revision).
pub const SIM_UNIT_CLASS_ID: u8 = 0xF0;

/// Revision of the [`SIM_UNIT_CLASS_ID`] chunk payload (append-only).
///
/// v2 appends the carried payload stack (`UnitPayloads.stack`: `u16 count` then
/// per payload `u8 tag` + fields). v3 appends the unit-save completeness fields
/// (H04 T9): `spawnedByCore`, `elevation`, `shield`, the carried item stack,
/// statuses, the command id + move target and the builder plan queue. v4
/// (L06 T4) appends `TypeIO.writeAbilities` (`u8 len` + `f data` per ability,
/// TypeIO.java:446-451) and `UnitTetherComp.spawnerUnitId` (`i32`, -1 = none,
/// UnitTetherComp.java:17). Older saves (without those fields) still load.
pub const SIM_UNIT_REVISION: u16 = 4;

/// Reserved class id for the native live-sim fire chunk (`FireComp`).
pub const SIM_FIRE_CLASS_ID: u8 = 0xF1;

/// Revision of the [`SIM_FIRE_CLASS_ID`] chunk payload (append-only).
pub const SIM_FIRE_REVISION: u16 = 1;

/// Reserved class id for the native live-sim puddle chunk (`PuddleComp`).
pub const SIM_PUDDLE_CLASS_ID: u8 = 0xF2;

/// Revision of the [`SIM_PUDDLE_CLASS_ID`] chunk payload (append-only).
pub const SIM_PUDDLE_REVISION: u16 = 1;

/// Host-supplied playtime for the `meta` region (`Saves.getTotalPlaytime`).
///
/// Playtime is host bookkeeping (`game::saves::Saves`), not sim state, so the
/// host stashes it here before the `IoSet::Capture` boundary.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SavePlaytime {
    /// Accumulated playtime in milliseconds.
    pub millis: i64,
}

impl Sim {
    /// Stores host playtime for the next save (`Saves.getTotalPlaytime`).
    pub fn set_save_playtime(&mut self, millis: i64) {
        if let Some(mut source) = self.ecs.0.get_resource_mut::<SavePlaytime>() {
            source.millis = millis;
        } else {
            self.ecs.0.insert_resource(SavePlaytime { millis });
        }
    }

    /// Stored playtime in milliseconds (`0` when the host never set it).
    pub fn save_playtime_ms(&self) -> i64 {
        self.ecs
            .0
            .get_resource::<SavePlaytime>()
            .map_or(0, |source| source.millis)
    }
}

/// One decoded live-unit record (`SaveVersion.writeWorldEntities`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DecodedUnit {
    /// Chunk revision the unit was read from.
    pub(crate) revision: u16,
    /// Saved entity id (chunk header; Java `writeWorldEntities` writes
    /// `entity.id()` after the class id). Restored onto the respawned unit by
    /// `apply_decoded_entities` (I01 T10). Negative = no id.
    pub(crate) saved_id: i32,
    /// Content unit name (`UnitType.name`).
    pub(crate) name: String,
    /// Team id.
    pub(crate) team: u8,
    /// World-pixel x.
    pub(crate) x: f32,
    /// World-pixel y.
    pub(crate) y: f32,
    /// Facing angle in degrees.
    pub(crate) rotation: f32,
    /// Current health.
    pub(crate) health: f32,
    /// Carried payload stack (revision 2+; empty in v1 saves).
    pub(crate) payloads: Vec<crate::entities::comp::unit::comp::CarriedPayload>,
    /// `UnitComp.spawnedByCore` (revision 3+).
    pub(crate) spawned_by_core: bool,
    /// Flight elevation (revision 3+).
    pub(crate) elevation: f32,
    /// Current shield points (revision 3+).
    pub(crate) shield: f32,
    /// Carried item stack (revision 3+).
    pub(crate) stack: Option<(crate::content::ItemId, i32)>,
    /// Active status effects (revision 3+).
    pub(crate) statuses: Vec<crate::entities::comp::unit::comp::StatusEntry>,
    /// Active command id (revision 3+).
    pub(crate) command: Option<u16>,
    /// Saved controller kind (revision 3+).
    pub(crate) controller: Option<crate::ai::controller::AiKind>,
    /// Builder plan queue (revision 3+).
    pub(crate) plans: Vec<crate::sim::builder::UnitBuildPlan>,
    /// `Ability.data` per ability, in write order (revision 4+;
    /// `TypeIO.readAbilities`).
    pub(crate) ability_data: Vec<f32>,
    /// `UnitTetherComp.spawnerUnitId` (revision 4+; -1 = none).
    pub(crate) spawner_unit_id: i32,
}

/// One decoded live fire record (`FireComp.read` sim fields).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DecodedFire {
    /// Owning tile.
    pub(crate) tile: (i16, i16),
    /// Ticks lived.
    pub(crate) time: f32,
    /// Ticks to live.
    pub(crate) lifetime: f32,
}

/// One decoded live puddle record (`PuddleComp.read` sim fields).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DecodedPuddle {
    /// Owning tile.
    pub(crate) tile: (i16, i16),
    /// Liquid content id.
    pub(crate) liquid: crate::content::LiquidId,
    /// Current amount.
    pub(crate) amount: f32,
}
