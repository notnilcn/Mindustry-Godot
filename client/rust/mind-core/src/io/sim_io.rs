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

use std::path::Path;

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;

use crate::content::{
    BlockKind, ContentError, ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content,
};
use crate::ecs::{BuildingComp, EntitySeq};
use crate::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
use crate::entities::comp::{Building, Health, Pos, TeamComp};
use crate::game::map_markers::MapMarkers;
use crate::game::runtime::CampaignRuntime;
use crate::game::teams::{BlockPlan, Teams};
use crate::io::fs::{FileSystem, NativeFs};
use crate::io::save::chunk::SaveScratch;
use crate::io::save::state::{EntitySink, EntitySource, MapSource, MarkersIo, TeamPlan};
use crate::io::save::versions::v1::base_meta_tags;
use crate::io::save::{SaveIo, SaveOptions, SaveReadState, WriteContext};
use crate::io::wire::{WireReader, WireWriter};
use crate::io::{IoError, IoResult, StringMap};
use crate::sim::unit_runtime::UnitRuntime;
use crate::sim::{IoHandler, IoRequest, IoResponse, IoStatus, Sim};
use crate::world::config::{ConfigValue, config_to_type_value, type_value_to_config};
use crate::world::{EcsMapSource, WorldGrid};

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
/// statuses, the command id + move target and the builder plan queue. v1/v2
/// saves (without those fields) still load.
pub const SIM_UNIT_REVISION: u16 = 3;

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

/// Read-side entity sink collecting the native live-unit/fire/puddle chunks.
#[derive(Default)]
struct LiveEntitySink {
    units: Vec<DecodedUnit>,
    fires: Vec<DecodedFire>,
    puddles: Vec<DecodedPuddle>,
}

impl EntitySink for LiveEntitySink {
    fn supports_class(&self, class_id: u8, custom_name: Option<&str>) -> bool {
        if matches!(
            class_id,
            SIM_UNIT_CLASS_ID | SIM_FIRE_CLASS_ID | SIM_PUDDLE_CLASS_ID
        ) {
            return true;
        }
        // H04 T10: upstream (Java `MSAV`) classes. A custom id-map name means
        // a mod entity reusing the byte; only the vanilla classes decode.
        custom_name.is_none() && crate::io::java_entities::supports_java_class(class_id)
    }

    fn read_entity(
        &mut self,
        class_id: u8,
        _custom_name: Option<&str>,
        id: i32,
        content: Option<&ContentRegistry>,
        r: &mut WireReader,
    ) -> IoResult<()> {
        match class_id {
            SIM_UNIT_CLASS_ID => {
                let revision = r.us()?;
                if revision == 0 || revision > SIM_UNIT_REVISION {
                    return Err(IoError::corrupt(format!(
                        "unsupported live unit revision {revision}"
                    )));
                }
                let mut decoded = DecodedUnit {
                    revision,
                    saved_id: id,
                    name: r.str()?,
                    team: r.ub()?,
                    x: r.f()?,
                    y: r.f()?,
                    rotation: r.f()?,
                    health: r.f()?,
                    payloads: Vec::new(),
                    spawned_by_core: false,
                    elevation: 0.0,
                    shield: 0.0,
                    stack: None,
                    statuses: Vec::new(),
                    command: None,
                    controller: None,
                    plans: Vec::new(),
                };
                if revision >= 2 {
                    let count = r.us()?;
                    for _ in 0..count {
                        let tag = r.ub()?;
                        let payload = match tag {
                            0 => crate::entities::comp::unit::comp::CarriedPayload::Unit {
                                type_id: crate::content::UnitTypeId::new(r.us()?),
                                team: r.ub()?,
                                health: r.f()?,
                                rotation: r.f()?,
                            },
                            1 => crate::entities::comp::unit::comp::CarriedPayload::Build {
                                block: crate::content::BlockId::new(r.us()?),
                                team: r.ub()?,
                                rotation: r.ub()?,
                                health: r.f()?,
                            },
                            other => {
                                return Err(IoError::corrupt(format!(
                                    "unknown unit payload tag {other}"
                                )));
                            }
                        };
                        decoded.payloads.push(payload);
                    }
                }
                if revision >= 3 {
                    // H04 T9: unit-save completeness (pulled from
                    // `SaveVersion.java:425-448` + the unit manifests).
                    decoded.spawned_by_core = r.bool()?;
                    decoded.elevation = r.f()?;
                    decoded.shield = r.f()?;
                    let stack_item = r.s()?;
                    let stack_amount = r.i()?;
                    if stack_item >= 0 {
                        decoded.stack =
                            Some((crate::content::ItemId::new(stack_item as u16), stack_amount));
                    }
                    let status_count = r.i()?.clamp(0, 1024) as usize;
                    for _ in 0..status_count {
                        let effect = r.s()?;
                        let duration = r.f()?;
                        if effect >= 0 {
                            decoded.statuses.push(
                                crate::entities::comp::unit::comp::StatusEntry::new(
                                    crate::content::StatusId::new(effect as u16),
                                    duration,
                                ),
                            );
                        }
                    }
                    let command = r.s()?;
                    decoded.command = (command >= 0).then_some(command as u16);
                    decoded.controller = ai_kind_from_code(r.ub()?);
                    let plan_count = r.i()?.clamp(0, 1024) as usize;
                    for _ in 0..plan_count {
                        let plan = crate::io::typeio::codecs::read_plan(r)?;
                        decoded.plans.push(unit_plan_from_wire(plan));
                    }
                }
                self.units.push(decoded);
            }
            SIM_FIRE_CLASS_ID => {
                let revision = r.us()?;
                if revision != SIM_FIRE_REVISION {
                    return Err(IoError::corrupt(format!(
                        "unsupported live fire revision {revision}"
                    )));
                }
                let mut state = crate::combat::fires::FireState {
                    tile: (0, 0),
                    time: 0.0,
                    lifetime: 0.0,
                    puddle_flammability: 0.0,
                    spread_timer: 0.0,
                    fireball_timer: 0.0,
                    damage_timer: 0.0,
                    warmup: 0.0,
                    animation: 0.0,
                };
                crate::combat::fires::read_fire(&mut state, r, revision as u8)?;
                self.fires.push(DecodedFire {
                    tile: state.tile,
                    time: state.time,
                    lifetime: state.lifetime,
                });
            }
            SIM_PUDDLE_CLASS_ID => {
                let revision = r.us()?;
                if revision != SIM_PUDDLE_REVISION {
                    return Err(IoError::corrupt(format!(
                        "unsupported live puddle revision {revision}"
                    )));
                }
                let mut state = crate::combat::puddles::PuddleState {
                    tile: (0, 0),
                    liquid: crate::content::LiquidId::WATER,
                    amount: 0.0,
                    accepting: 0.0,
                    update_time: 0.0,
                    last_ripple: 0.0,
                    effect_time: 0.0,
                };
                crate::combat::puddles::read_puddle(&mut state, r, revision as u8)?;
                self.puddles.push(DecodedPuddle {
                    tile: state.tile,
                    liquid: state.liquid,
                    amount: state.amount,
                });
            }
            // H04 T10: upstream `FireComp`/`PuddleComp`/unit chunks
            // (`supports_class` only routes the decodable Java classes here).
            crate::io::java_entities::JAVA_FIRE_CLASS_ID => {
                if let Some(fire) = crate::io::java_entities::read_java_fire(r)? {
                    self.fires.push(fire);
                }
            }
            crate::io::java_entities::JAVA_PUDDLE_CLASS_ID => {
                let Some(content) = content else {
                    return Ok(());
                };
                if let Some(puddle) = crate::io::java_entities::read_java_puddle(r, content)? {
                    self.puddles.push(puddle);
                }
            }
            other => {
                let Some(content) = content else {
                    return Ok(());
                };
                if let Some(unit) = crate::io::java_entities::read_java_unit(other, id, r, content)?
                {
                    self.units.push(unit);
                }
            }
        }
        Ok(())
    }

    fn after_read_all(&mut self) {}
}

/// Write-side entity source over the live `UnitRuntime` and campaign teams.
struct LiveEntitySource<'a> {
    sim: &'a Sim,
}

impl EntitySource for LiveEntitySource<'_> {
    fn team_plans(&self) -> Vec<(i32, Vec<TeamPlan>)> {
        let Some(runtime) = self.sim.ecs.0.get_resource::<CampaignRuntime>() else {
            return Vec::new();
        };
        let teams = &runtime.session.teams;
        teams
            .active
            .iter()
            .filter_map(|team| {
                let data = teams.get_or_null(*team)?;
                if data.plans.is_empty() {
                    return None;
                }
                let plans = data
                    .plans
                    .iter()
                    .map(|plan| TeamPlan {
                        x: plan.x,
                        y: plan.y,
                        rotation: i16::from(plan.rotation),
                        block: plan.block,
                        config: config_to_type_value(
                            plan.config.as_ref().unwrap_or(&ConfigValue::None),
                        ),
                    })
                    .collect();
                Some((i32::from(team.0), plans))
            })
            .collect()
    }

    fn entity_count(&self) -> usize {
        // H04 T9: the count must equal the chunks `write_entities` emits
        // (`SaveVersion.java:425-439`); the old `unit_count` counted units the
        // writer skips, so the reader ate fire/puddle bytes as unit chunks.
        let units = live_unit_rows(self.sim).len();
        let mut fires = 0usize;
        let mut puddles = 0usize;
        for entity in self.sim.ecs.0.iter_entities() {
            if entity.get::<crate::combat::fires::FireState>().is_some() {
                fires += 1;
            }
            if entity
                .get::<crate::combat::puddles::PuddleState>()
                .is_some()
            {
                puddles += 1;
            }
        }
        units + fires + puddles
    }

    /// Units `write_entities` will actually write, in `(seq, entity, name)`
    /// order (live entity + `UnitCore` + `UnitTypeComp` + `Pos` + a registered
    /// name; H04 T9 `SaveVersion.writeWorldEntities` count).
    fn write_entities(&self, w: &mut WireWriter, scratch: &mut SaveScratch) -> IoResult<()> {
        let world = &self.sim.ecs.0;
        if world.get_non_send::<UnitRuntime>().is_none() {
            return Ok(());
        }
        for (seq, entity, name) in live_unit_rows(self.sim) {
            let (Some(core), Some(pos)) = (world.get::<UnitCore>(entity), world.get::<Pos>(entity))
            else {
                continue;
            };
            let team = world.get::<TeamComp>(entity).map(|t| t.team).unwrap_or(0);
            let health = world.get::<Health>(entity).map(|h| h.health).unwrap_or(0.0);
            scratch.write_chunk(w, |chunk| {
                chunk.ub(SIM_UNIT_CLASS_ID);
                // `SaveVersion.writeEntity`: `out.i(entity.id())` — the entity
                // id is part of the chunk header and restored on load (I01 T10).
                chunk.i(seq as i32);
                chunk.us(SIM_UNIT_REVISION);
                chunk.str(&name)?;
                chunk.ub(team);
                chunk.f(pos.x);
                chunk.f(pos.y);
                chunk.f(core.rotation);
                chunk.f(health);
                // Revision 2: carried payload stack (`UnitPayloads.stack`).
                let payloads = world
                    .get::<crate::entities::comp::unit::comp::UnitPayloads>(entity)
                    .map(|payloads| payloads.stack.as_slice())
                    .unwrap_or(&[]);
                chunk.us(payloads.len().min(u16::MAX as usize) as u16);
                for payload in payloads {
                    match payload {
                        crate::entities::comp::unit::comp::CarriedPayload::Unit {
                            type_id,
                            team,
                            health,
                            rotation,
                        } => {
                            chunk.ub(0);
                            chunk.us(type_id.raw());
                            chunk.ub(*team);
                            chunk.f(*health);
                            chunk.f(*rotation);
                        }
                        crate::entities::comp::unit::comp::CarriedPayload::Build {
                            block,
                            team,
                            rotation,
                            health,
                        } => {
                            chunk.ub(1);
                            chunk.us(block.raw());
                            chunk.ub(*team);
                            chunk.ub(*rotation);
                            chunk.f(*health);
                        }
                    }
                }
                // Revision 3 (H04 T9): unit-save completeness.
                chunk.bool(core.spawned_by_core);
                chunk.f(core.elevation);
                let shield = world
                    .get::<crate::entities::comp::unit::comp::ShieldComp>(entity)
                    .map(|shield| shield.shield)
                    .unwrap_or(0.0);
                chunk.f(shield);
                let stack = world
                    .get::<crate::entities::comp::unit::comp::ItemsComp>(entity)
                    .and_then(|items| items.item);
                chunk.s(stack.map(|(item, _)| item.raw() as i16).unwrap_or(-1));
                chunk.i(stack.map(|(_, amount)| amount).unwrap_or(0));
                let statuses = world
                    .get::<crate::entities::comp::unit::comp::StatusComp>(entity)
                    .map(|status| status.statuses.clone())
                    .unwrap_or_default();
                chunk.i(statuses.len().min(i32::MAX as usize) as i32);
                for entry in &statuses {
                    chunk.s(entry.effect.raw() as i16);
                    chunk.f(entry.duration);
                }
                let command = world
                    .get::<crate::ai::controller::UnitCommandComp>(entity)
                    .map(|command| command.0 as i16)
                    .unwrap_or(-1);
                chunk.s(command);
                // Controller kind (`UnitComp.afterRead` -> `createController`;
                // T13 keeps Logic/Command state and resets the rest).
                let controller = world
                    .get::<crate::ai::controller::ControllerSlot>(entity)
                    .map(|slot| ai_kind_code(slot.kind))
                    .unwrap_or(255);
                chunk.ub(controller);
                let plans: Vec<crate::io::typeio::codecs::BuildPlan> = world
                    .get::<crate::sim::builder::BuilderPlans>(entity)
                    .map(|builder| builder.plans.iter().map(unit_plan_to_wire).collect())
                    .unwrap_or_default();
                chunk.i(plans.len().min(i32::MAX as usize) as i32);
                for plan in &plans {
                    crate::io::typeio::codecs::write_plan(chunk, plan)?;
                }
                Ok(())
            })?;
        }
        // Fires (`FireComp.write`: tile/time/lifetime) then puddles
        // (`PuddleComp.write`: amount/tile/liquid), both in tile order (they
        // carry no `EntitySeq`).
        let mut fires: Vec<crate::combat::fires::FireState> = world
            .iter_entities()
            .filter_map(|entity| entity.get::<crate::combat::fires::FireState>().copied())
            .collect();
        fires.sort_by_key(|state| (state.tile.0, state.tile.1));
        for state in fires {
            scratch.write_chunk(w, |chunk| {
                chunk.ub(SIM_FIRE_CLASS_ID);
                chunk.i(0);
                chunk.us(SIM_FIRE_REVISION);
                crate::combat::fires::write_fire(&state, chunk)?;
                Ok(())
            })?;
        }
        let mut puddles: Vec<crate::combat::puddles::PuddleState> = world
            .iter_entities()
            .filter_map(|entity| entity.get::<crate::combat::puddles::PuddleState>().copied())
            .collect();
        puddles.sort_by_key(|state| (state.tile.0, state.tile.1));
        for state in puddles {
            scratch.write_chunk(w, |chunk| {
                chunk.ub(SIM_PUDDLE_CLASS_ID);
                chunk.i(0);
                chunk.us(SIM_PUDDLE_REVISION);
                crate::combat::puddles::write_puddle(&state, chunk)?;
                Ok(())
            })?;
        }
        Ok(())
    }
}

/// Stable save code for [`crate::ai::controller::AiKind`] (revision 3;
/// append-only).
fn ai_kind_code(kind: crate::ai::controller::AiKind) -> u8 {
    use crate::ai::controller::AiKind;
    match kind {
        AiKind::NoAi => 0,
        AiKind::Ground => 1,
        AiKind::Hug => 2,
        AiKind::Flying => 3,
        AiKind::FlyingFollow => 4,
        AiKind::Command => 5,
        AiKind::Logic => 6,
        AiKind::Missile => 7,
        AiKind::Builder => 8,
        AiKind::Miner => 9,
        AiKind::Cargo => 10,
        AiKind::Defender => 11,
        AiKind::Suicide => 12,
        AiKind::Boost => 13,
        AiKind::Assembler => 14,
        AiKind::Prebuild => 15,
        AiKind::Repair => 16,
        AiKind::Player => 17,
    }
}

fn ai_kind_from_code(code: u8) -> Option<crate::ai::controller::AiKind> {
    use crate::ai::controller::AiKind;
    Some(match code {
        0 => AiKind::NoAi,
        1 => AiKind::Ground,
        2 => AiKind::Hug,
        3 => AiKind::Flying,
        4 => AiKind::FlyingFollow,
        5 => AiKind::Command,
        6 => AiKind::Logic,
        7 => AiKind::Missile,
        8 => AiKind::Builder,
        9 => AiKind::Miner,
        10 => AiKind::Cargo,
        11 => AiKind::Defender,
        12 => AiKind::Suicide,
        13 => AiKind::Boost,
        14 => AiKind::Assembler,
        15 => AiKind::Prebuild,
        16 => AiKind::Repair,
        17 => AiKind::Player,
        _ => return None,
    })
}

/// Wire plan -> builder plan (`TypeIO.readPlan`).
fn unit_plan_from_wire(
    plan: crate::io::typeio::codecs::BuildPlan,
) -> crate::sim::builder::UnitBuildPlan {
    crate::sim::builder::UnitBuildPlan {
        x: plan.x as i16,
        y: plan.y as i16,
        rotation: plan.rotation as u8,
        block: plan.block,
        config: type_value_to_config(&plan.config),
        breaking: plan.breaking,
        initialized: false,
        stuck: false,
        progress: 0.0,
    }
}

/// Builder plan -> wire plan (`TypeIO.writePlan`).
fn unit_plan_to_wire(
    plan: &crate::sim::builder::UnitBuildPlan,
) -> crate::io::typeio::codecs::BuildPlan {
    crate::io::typeio::codecs::BuildPlan {
        x: i32::from(plan.x),
        y: i32::from(plan.y),
        rotation: i32::from(plan.rotation),
        block: plan.block,
        config: config_to_type_value(&plan.config),
        breaking: plan.breaking,
    }
}

/// Units `write_entities` will actually write, in `(seq, entity, name)` order
/// (live entity + `UnitCore` + `UnitTypeComp` + `Pos` + a registered name;
/// H04 T9 `SaveVersion.writeWorldEntities` count).
fn live_unit_rows(sim: &Sim) -> Vec<(u64, Entity, String)> {
    let world = &sim.ecs.0;
    let Some(runtime) = world.get_non_send::<UnitRuntime>() else {
        return Vec::new();
    };
    let mut rows: Vec<(u64, Entity, String)> = runtime
        .units
        .iter()
        .copied()
        .filter(|entity| world.get_entity(*entity).is_ok())
        .filter_map(|entity| {
            let unit = world.get::<UnitTypeComp>(entity)?;
            if world.get::<UnitCore>(entity).is_none() || world.get::<Pos>(entity).is_none() {
                return None;
            }
            let name = runtime
                .content
                .unit(unit.type_id)
                .map(|def| def.name.clone())?;
            let seq = world
                .get::<EntitySeq>(entity)
                .map(|seq| seq.0)
                .unwrap_or(u64::MAX);
            Some((seq, entity, name))
        })
        .collect();
    rows.sort_by_key(|(seq, entity, _)| (*seq, entity.index()));
    rows
}

/// Restores decoded team build plans onto the live session (`readTeamBlocks`).
pub fn apply_team_plans(teams: &mut Teams, plans: Vec<(i32, Vec<TeamPlan>)>) {
    for (team, team_plans) in plans {
        let Ok(team) = u8::try_from(team) else {
            continue;
        };
        let data = teams.get(crate::ecs::TeamId(team));
        data.plans.clear();
        for plan in team_plans {
            data.plans.push_back(BlockPlan {
                x: plan.x,
                y: plan.y,
                rotation: plan.rotation as i8,
                block: plan.block,
                config: Some(type_value_to_config(&plan.config)),
                removed: false,
            });
        }
    }
}

/// One decoded save (`SaveIO.load` output before it touches the world).
pub struct DecodedSave {
    /// The decoded tile grid.
    pub grid: WorldGrid,
    /// Decoded building payloads awaiting ECS spawn.
    pub pending: Vec<(usize, crate::world::building_io::DecodedBase)>,
    /// Team build plans read from the `entities` region.
    pub plans: Vec<(i32, Vec<TeamPlan>)>,
    /// Meta tags as read (`SaveReadState.tags`).
    pub tags: StringMap,
    /// Parsed rules (`SaveVersion.readRules`).
    pub rules: Option<crate::io::json::Rules>,
    /// Per-game stats from the `meta` region.
    pub stats: Option<crate::io::json::GameStats>,
    /// Map markers (`MapMarkers::read`).
    pub markers: MapMarkers,
    /// Decoded world entities (units/fires/puddles).
    entities: LiveEntitySink,
    /// `static-fog-data` raw chunk bytes (`FogControl.write`), applied after
    /// the world rebuild (`SaveVersion.readCustomChunks`).
    pub static_fog: Option<Vec<u8>>,
}

/// Decodes one save/map container into [`DecodedSave`] without touching the
/// live sim (`SaveIO.load`'s world half + `World.endMapLoad`).
///
/// Used by both the Load-Game path and the sector-resume path so a resumed
/// sector restores exactly what a loaded save does (`readMeta`, `readMap`,
/// `readEntities`, `readMarkers`).
pub fn decode_save_bytes(
    bytes: &[u8],
    registry: &mut ContentRegistry,
) -> Result<DecodedSave, IoError> {
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let mut entities = LiveEntitySink::default();
    let mut markers = MapMarkers::new();
    // `static-fog-data` (`FogControl.java:97`): registered so the reader's
    // custom-chunk pass collects it instead of skipping it.
    let static_fog_chunk = std::sync::Arc::new(crate::game::fog::StaticFogChunk(
        std::sync::Mutex::new(None),
    ));
    let mut custom_chunks: indexmap::IndexMap<
        String,
        std::sync::Arc<dyn crate::io::save::state::CustomChunk>,
    > = indexmap::IndexMap::new();
    custom_chunks.insert(
        "static-fog-data".to_owned(),
        static_fog_chunk.clone() as std::sync::Arc<dyn crate::io::save::state::CustomChunk>,
    );
    let (result, tags, rules, stats, plans, pending) = {
        let mut context = crate::editor::context::EditorContext::new(&mut grid, registry);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(registry),
            entities: Some(&mut entities),
            markers: Some(&mut markers),
            custom_chunks: Some(&custom_chunks),
            ..SaveReadState::default()
        };
        let result = SaveIo::load_bytes(bytes, &mut state);
        (
            result,
            state.tags.clone(),
            state.rules.take(),
            state.stats.take(),
            std::mem::take(&mut state.team_plans),
            context.take_pending_buildings(),
        )
    };
    result?;
    grid.end_map_load(registry);
    let static_fog = static_fog_chunk
        .0
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    Ok(DecodedSave {
        grid,
        pending,
        plans,
        tags,
        rules,
        stats,
        markers,
        entities,
        static_fog,
    })
}

/// `SaveVersion.readMeta` tick: Java writes `state.tick` as a float
/// (`SaveVersion.java:135`; e.g. `74802.1339981854` in the r05 fixture), so
/// parse as `f64` and floor to ticks.
pub fn meta_tick(tags: &StringMap) -> Option<u64> {
    tags.get("tick")
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|tick| tick.is_finite())
        .map(|tick| tick.max(0.0).floor() as u64)
}

/// Resume metadata a sector load hands to the campaign facade after
/// `play_sector` (the `readMeta` fields the rules do not carry).
#[derive(Debug, Clone, Default)]
pub struct SectorResumeMeta {
    /// `wavetime` meta tag (ticks).
    pub wavetime: Option<f32>,
    /// `tick` meta tag (Java float text).
    pub tick: Option<u64>,
    /// Saved `GameStats`.
    pub stats: Option<crate::io::json::GameStats>,
    /// Saved map markers.
    pub markers: MapMarkers,
    /// `static-fog-data` raw chunk bytes (`FogControl.write`), applied by the
    /// host after the rules adoption (`DecodedSave.static_fog`).
    pub static_fog: Option<Vec<u8>>,
}

impl Sim {
    /// Respawns the decoded world entities (units, fires, puddles) into the
    /// live runtime (`SaveVersion.readWorldEntities` application half).
    pub fn apply_decoded_entities(&mut self, save: &DecodedSave) {
        // H04 T13: remember which units this restore spawned so
        // `reset_loaded_controllers` can run Java's `UnitComp.afterRead` once
        // the session rules reach the world.
        self.loaded_unit_entities.clear();
        // I01 T10: `(entity, saved id)` pairs, restored after the spawn pass
        // (`SaveVersion.readWorldEntities` checks every id before reassigning
        // duplicates).
        let mut spawned: Vec<(Entity, i32)> = Vec::new();
        let default_team = self
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .map(|runtime| runtime.session.rules.default_team)
            .unwrap_or_else(|| crate::game::rules::Rules::default().default_team);
        for unit in &save.entities.units {
            // `UnitComp.java:868-871` despawns a `spawnedByCore` unit that is
            // not the player's; the host spawns a fresh player unit
            // (`spawn_player_unit`). Revision 3 carries the real flag; v1/v2
            // saves fall back to the core unit type names (`Blocks.java`
            // `coreShard..coreAcropolis` `unitType`).
            const CORE_UNITS: [&str; 6] = ["alpha", "beta", "gamma", "evoke", "incite", "emanate"];
            let core_unit_by_name = unit.revision < 3;
            if unit.team == default_team
                && (unit.spawned_by_core
                    || (core_unit_by_name && CORE_UNITS.contains(&unit.name.as_str())))
            {
                continue;
            }
            if let Some(entity) =
                self.spawn_live_unit(&unit.name, unit.team, unit.x, unit.y, unit.rotation)
            {
                self.loaded_unit_entities.push(entity);
                spawned.push((entity, unit.saved_id));
                if unit.health > 0.0
                    && let Some(mut health) = self.ecs.0.get_mut::<Health>(entity)
                {
                    health.health = unit.health;
                }
                // Revision 3: core spawn flag + elevation.
                if let Some(mut core) = self.ecs.0.get_mut::<UnitCore>(entity) {
                    core.spawned_by_core = unit.spawned_by_core;
                    core.elevation = unit.elevation;
                }
                if unit.shield > 0.0 {
                    if let Some(mut shield) =
                        self.ecs
                            .0
                            .get_mut::<crate::entities::comp::unit::comp::ShieldComp>(entity)
                    {
                        shield.shield = unit.shield;
                    } else {
                        self.ecs.0.entity_mut(entity).insert(
                            crate::entities::comp::unit::comp::ShieldComp {
                                shield: unit.shield,
                                ..Default::default()
                            },
                        );
                    }
                }
                if let Some((item, amount)) = unit.stack {
                    if let Some(mut items) = self
                        .ecs
                        .0
                        .get_mut::<crate::entities::comp::unit::comp::ItemsComp>(entity)
                    {
                        items.item = Some((item, amount));
                    } else {
                        self.ecs.0.entity_mut(entity).insert(
                            crate::entities::comp::unit::comp::ItemsComp {
                                item: Some((item, amount)),
                            },
                        );
                    }
                }
                if !unit.statuses.is_empty() {
                    if let Some(mut status) =
                        self.ecs
                            .0
                            .get_mut::<crate::entities::comp::unit::comp::StatusComp>(entity)
                    {
                        status.statuses = unit.statuses.clone();
                        status.damage_times.resize(unit.statuses.len(), 0.0);
                    } else {
                        let len = unit.statuses.len();
                        self.ecs.0.entity_mut(entity).insert(
                            crate::entities::comp::unit::comp::StatusComp {
                                statuses: unit.statuses.clone(),
                                damage_times: vec![0.0; len],
                                ..Default::default()
                            },
                        );
                    }
                }
                if let Some(command) = unit.command {
                    self.ecs
                        .0
                        .entity_mut(entity)
                        .insert(crate::ai::controller::UnitCommandComp(command));
                }
                if let Some(kind) = unit.controller
                    && let Some(mut slot) = self
                        .ecs
                        .0
                        .get_mut::<crate::ai::controller::ControllerSlot>(entity)
                {
                    slot.kind = kind;
                }
                if !unit.plans.is_empty() {
                    let plans: std::collections::VecDeque<crate::sim::builder::UnitBuildPlan> =
                        unit.plans.iter().cloned().collect();
                    if let Some(mut builder) = self
                        .ecs
                        .0
                        .get_mut::<crate::sim::builder::BuilderPlans>(entity)
                    {
                        builder.plans = plans;
                    } else {
                        self.ecs
                            .0
                            .entity_mut(entity)
                            .insert(crate::sim::builder::BuilderPlans {
                                plans,
                                ..Default::default()
                            });
                    }
                }
                // Revision 2: restore the carried payload stack.
                if !unit.payloads.is_empty() {
                    let stack = unit.payloads.clone();
                    if let Some(mut payloads) =
                        self.ecs
                            .0
                            .get_mut::<crate::entities::comp::unit::comp::UnitPayloads>(entity)
                    {
                        payloads.stack = stack;
                    } else {
                        self.ecs
                            .0
                            .entity_mut(entity)
                            .insert(crate::entities::comp::unit::comp::UnitPayloads { stack });
                    }
                }
            }
        }
        self.restore_unit_ids(&spawned);
        // Fires and puddles (`Groups.fires`/`Groups.puddles` recreate from
        // their chunks; the tile-slot caches re-register on the next update
        // pass). Accumulator fields are transient upstream and start zeroed.
        let tile_size = crate::config::TILESIZE as f32;
        for fire in &save.entities.fires {
            self.ecs.0.spawn((
                Pos {
                    x: (fire.tile.0 as f32 + 0.5) * tile_size,
                    y: (fire.tile.1 as f32 + 0.5) * tile_size,
                },
                TeamComp { team: 0 },
                crate::combat::fires::FireState {
                    tile: fire.tile,
                    time: fire.time,
                    lifetime: fire.lifetime,
                    puddle_flammability: 0.0,
                    spread_timer: 0.0,
                    fireball_timer: 0.0,
                    damage_timer: 0.0,
                    warmup: 0.0,
                    animation: 0.0,
                },
            ));
        }
        for puddle in &save.entities.puddles {
            self.ecs.0.spawn((
                Pos {
                    x: (puddle.tile.0 as f32 + 0.5) * tile_size,
                    y: (puddle.tile.1 as f32 + 0.5) * tile_size,
                },
                crate::combat::puddles::PuddleState {
                    tile: puddle.tile,
                    liquid: puddle.liquid,
                    amount: puddle.amount,
                    accepting: 0.0,
                    update_time: 0.0,
                    last_ripple: 0.0,
                    effect_time: 0.0,
                },
            ));
        }
    }

    /// Restores the saved `EntitySeq` ids onto the respawned units
    /// (`SaveVersion.readWorldEntities`, `SaveVersion.java:495-528`):
    /// `EntityGroup.checkNextId(id); entity.id(id); if(used.add(id)) add();
    /// else reassign(nextId())`. The advanced ids guarantee that later unit
    /// spawns (`UnitRuntime.seq`, block outputs) never collide with a restored
    /// unit, so `unit_by_id` stays unambiguous (I01 T10).
    fn restore_unit_ids(&mut self, spawned: &[(Entity, i32)]) {
        if spawned.is_empty() {
            return;
        }
        let mut used: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
        let mut max_id: Option<u64> = None;
        let mut reassign: Vec<Entity> = Vec::new();
        for (entity, saved_id) in spawned {
            if *saved_id >= 0 && used.insert(*saved_id as u64) {
                if let Some(mut seq) = self.ecs.0.get_mut::<EntitySeq>(*entity) {
                    seq.0 = *saved_id as u64;
                }
                max_id = Some(max_id.map_or(*saved_id as u64, |max| max.max(*saved_id as u64)));
            } else {
                // Duplicate id (or none saved): `EntityGroup.nextId()`.
                reassign.push(*entity);
            }
        }
        // Duplicates take the ids right after the highest restored one; the
        // allocators then continue past all of them (`EntityGroup.lastId`).
        let mut next = max_id.map_or(0, |max| max.saturating_add(1));
        for entity in reassign {
            if let Some(mut seq) = self.ecs.0.get_mut::<EntitySeq>(entity) {
                seq.0 = next;
            }
            next = next.wrapping_add(1);
        }
        if let Some(mut runtime) = self.ecs.0.remove_non_send::<UnitRuntime>() {
            next = next.max(runtime.seq);
            runtime.seq = next;
            self.ecs.0.insert_non_send(runtime);
        }
        // Block-output spawns (`UnitBlockSeq`) share the id space; the bump
        // inserts the resource (with its base floor) when absent.
        crate::world::blocks::units::behavior::UnitBlockSeq::bump_past(&mut self.ecs.0, max_id);
        // Future P0 building placements (`Command::Place`).
        self.bump_entity_seq(next);
    }

    /// `UnitComp.afterRead` (`UnitComp.java:585-593`): every unit read from a
    /// save re-derives `type.createController(unit)` unless its saved
    /// controller keeps state (`LogicAI`/`CommandAI`; `keepState`). The resume
    /// path calls this once the session rules are installed, because the
    /// world load spawns the units before `CampaignRuntime` exists and the
    /// spawn fallback (`BuildRules.defaultTeam` = derelict) marks a
    /// player-team unit as AI (`GroundAI` instead of `CommandAI`).
    pub fn reset_loaded_controllers(&mut self) {
        use crate::ai::controller::{AiKind, ControllerSlot};
        let entities = std::mem::take(&mut self.loaded_unit_entities);
        for entity in entities {
            let Some(kind) = self
                .ecs
                .0
                .get::<ControllerSlot>(entity)
                .map(|slot| slot.kind)
            else {
                continue;
            };
            // The player's own unit keeps its `Player` bridge; upstream keeps
            // the saved controller instance only when `keepState` is true.
            if kind == AiKind::Player || crate::ai::controller_registry::keep_state(kind) {
                continue;
            }
            let Some(team) = self.ecs.0.get::<TeamComp>(entity).map(|comp| comp.team) else {
                continue;
            };
            self.release_unit_to_ai(entity, team);
        }
    }
}

/// The live `meta` tag set (`SaveVersion.writeMeta`, `SaveVersion.java:114-151`).
fn live_meta_tags(
    sim: &Sim,
    content: &ContentRegistry,
    width: u16,
    height: u16,
    name: &str,
) -> StringMap {
    let session = sim
        .ecs
        .0
        .get_resource::<CampaignRuntime>()
        .map(|runtime| &runtime.session);
    let wave = session.map_or(0, |session| session.wave);
    let mut tags = base_meta_tags(width, height, wave, name);
    tags.insert("saved".to_owned(), sim.platform().millis().to_string());
    tags.insert("playtime".to_owned(), sim.save_playtime_ms().to_string());
    tags.insert("tick".to_owned(), sim.tick_count().to_string());
    if let Some(session) = session {
        tags.insert("wavetime".to_owned(), session.wavetime.to_string());
        tags.insert(
            "stats".to_owned(),
            serde_json::to_string(&session.stats).unwrap_or_else(|_| "{}".to_owned()),
        );
        tags.insert(
            "rules".to_owned(),
            serde_json::to_string(&session.rules).unwrap_or_else(|_| "{}".to_owned()),
        );
        tags.insert("playerteam".to_owned(), session.default_team().to_string());
        tags.insert(
            "nocores".to_owned(),
            (session.player_core_count() == 0).to_string(),
        );
    } else {
        // No live campaign session: classify the default team's cores from the
        // ECS (`state.rules.defaultTeam.cores().isEmpty()`).
        let default_team = crate::game::rules::Rules::default().default_team;
        let cores = sim
            .ecs
            .0
            .iter_entities()
            .filter(|entity| {
                entity.get::<BuildingComp>().is_some_and(|comp| {
                    comp.team.0 == default_team
                        && content
                            .block(comp.block)
                            .is_some_and(|def| def.kind == BlockKind::CoreBlock)
                })
            })
            .count();
        tags.insert("playerteam".to_owned(), default_team.to_string());
        tags.insert("nocores".to_owned(), (cores == 0).to_string());
    }
    tags
}

/// [`MapSource`] over the live grid that resolves multiblock centers from either
/// plan-07 [`Building`] entities or the P0 [`BuildingComp`] the base `Sim` uses.
///
/// [`EcsMapSource`] only knows the plan-07 `Building` component, so a P0 placed
/// block (which carries `BuildingComp`) would otherwise be written with its
/// entity chunk but skipped on read (`is_center` false).
struct SimMapSource<'a> {
    grid: &'a WorldGrid,
    inner: EcsMapSource<'a>,
}

impl<'a> SimMapSource<'a> {
    fn new(
        grid: &'a WorldGrid,
        content: &'a ContentRegistry,
        ecs: &'a bevy_ecs::world::World,
    ) -> Self {
        Self {
            grid,
            inner: EcsMapSource::new(grid, content, ecs),
        }
    }
}

impl MapSource for SimMapSource<'_> {
    fn width(&self) -> u16 {
        self.inner.width()
    }

    fn height(&self) -> u16 {
        self.inner.height()
    }

    fn floor_id(&self, index: usize) -> u16 {
        self.inner.floor_id(index)
    }

    fn overlay_id(&self, index: usize) -> u16 {
        self.inner.overlay_id(index)
    }

    fn block_id(&self, index: usize) -> u16 {
        self.inner.block_id(index)
    }

    fn has_building(&self, index: usize) -> bool {
        self.inner.has_building(index)
    }

    fn is_center(&self, index: usize) -> bool {
        let Some(entity) = self.grid.tiles.geti(index).build else {
            return true;
        };
        let width = self.width() as usize;
        let (x, y) = (index % width, index / width);
        if let Some(building) = self.inner.ecs.get::<Building>(entity) {
            return building.tile.x() as usize == x && building.tile.y() as usize == y;
        }
        if let Some(comp) = self.inner.ecs.get::<BuildingComp>(entity) {
            return comp.pos.x() as usize == x && comp.pos.y() as usize == y;
        }
        true
    }

    fn should_save_data(&self, index: usize) -> bool {
        self.inner.should_save_data(index)
    }

    fn tile_data(&self, index: usize) -> (u8, u8, u8, i32) {
        self.inner.tile_data(index)
    }

    fn write_building(&self, index: usize, chunk: &mut WireWriter) -> IoResult<()> {
        self.inner.write_building(index, chunk)
    }

    fn core_team(&self, index: usize) -> Option<u8> {
        self.inner.core_team(index)
    }
}

/// Live-sim IO executor over plan 04's native `MGRS` save format.
///
/// Content is booted per call and dropped afterwards: the save/load boundary is
/// rare, and keeping `ContentRegistry` off the struct keeps the handler `Send`
/// (the registry owns a non-`Send` `Box<dyn ModErrorSink>`, while `IoHandler`
/// requires `Send`).
pub struct SimIoHandler {
    /// Save knobs (native v1, embedded assets off).
    options: SaveOptions,
}

impl Default for SimIoHandler {
    fn default() -> Self {
        Self {
            options: SaveOptions::new(),
        }
    }
}

impl SimIoHandler {
    /// Creates a new handler.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a handler (content is booted on each IO call, so this never
    /// fails; the `Result` keeps a stable call-site contract with `mind-gdext`).
    pub fn boot() -> Result<Self, ContentError> {
        Ok(Self::new())
    }

    /// Captures one save request to `path`.
    fn save(&mut self, sim: &Sim, path: &Path, as_map: bool) -> IoStatus {
        let content = match boot_registry() {
            Ok(content) => content,
            Err(error) => return IoStatus::Failed(error.to_string()),
        };
        let width = sim.grid.tiles.width as u16;
        let height = sim.grid.tiles.height as u16;
        let name = if as_map { "map" } else { "sim" };
        let source = SimMapSource::new(&sim.grid, &content, &sim.ecs.0);
        let entities = LiveEntitySource { sim };
        let markers: Option<&dyn MarkersIo> = sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .map(|runtime| &runtime.session.markers as &dyn MarkersIo);
        let mut tags = live_meta_tags(sim, &content, width, height, name);
        tags.insert("name".to_owned(), name.to_owned());
        // `static-fog-data` (`FogControl.shouldWrite`): only when fog is on and
        // a controller with data exists.
        let mut fog_chunks: indexmap::IndexMap<
            String,
            std::sync::Arc<dyn crate::io::save::state::CustomChunk>,
        > = indexmap::IndexMap::new();
        if let Some(fog) = sim.ecs.0.get_resource::<crate::game::fog::FogControl>() {
            let (fog_enabled, static_fog) = crate::game::runtime::rules_of(&sim.ecs.0)
                .map(|rules| (rules.fog, rules.static_fog))
                .unwrap_or((false, false));
            if fog.should_write(fog_enabled, static_fog) {
                fog_chunks.insert(
                    "static-fog-data".to_owned(),
                    std::sync::Arc::new(crate::game::fog::StaticFogChunk(std::sync::Mutex::new(
                        Some(fog.write()),
                    )))
                        as std::sync::Arc<dyn crate::io::save::state::CustomChunk>,
                );
            }
        }
        let mut ctx = WriteContext::meta_only(tags);
        ctx.content = Some(&content);
        ctx.map = Some(&source);
        ctx.entities = Some(&entities);
        ctx.markers = markers;
        if !fog_chunks.is_empty() {
            ctx.custom_chunks = Some(&fog_chunks);
        }
        match SaveIo::save(&NativeFs, path, &ctx, &self.options) {
            Ok(()) => IoStatus::Ok,
            Err(error) => IoStatus::Failed(error.to_string()),
        }
    }

    /// Applies one load request from `path` onto the live world.
    ///
    /// The decoded buildings are materialized into the live block runtime, the
    /// saved units respawn into the unit runtime, and the decoded
    /// rules/wave/stats/markers/plans are applied to the installed campaign
    /// session (`SaveSlot.load`, `Saves.java:314-328`).
    fn load(&mut self, sim: &mut Sim, path: &Path) -> IoStatus {
        let mut content = match boot_registry() {
            Ok(content) => content,
            Err(error) => return IoStatus::Failed(error.to_string()),
        };
        let bytes = match NativeFs.read(path) {
            Ok(bytes) => bytes,
            Err(error) => return IoStatus::Failed(error.to_string()),
        };
        let save = match decode_save_bytes(&bytes, &mut content) {
            Ok(save) => save,
            Err(error) => return IoStatus::Failed(error.to_string()),
        };
        // The loaded header tick resumes the fixed-step counter (`readMeta`);
        // Java writes it as a float.
        if let Some(tick) = meta_tick(&save.tags) {
            sim.state.tick = tick;
        }
        // Replace the live world: drop the previous entities/bookkeeping, swap
        // the grid, then rebuild the ECS the tile model points at.
        sim.clear_world_for_load();
        sim.grid = save.grid.clone();
        let grid = sim.grid.clone();
        let core_capture =
            crate::game::runtime::rules_of(&sim.ecs.0).is_some_and(|rules| rules.core_capture);
        if let Some(mut unit_runtime) = sim.ecs.0.remove_non_send::<UnitRuntime>() {
            unit_runtime.reset(&grid, &sim.ecs.0, core_capture);
            sim.ecs.0.insert_non_send(unit_runtime);
        }
        let materialized = sim.materialize_map_buildings(&save.pending);
        if materialized > 0 {
            log::info!("sim_io load: materialized {materialized} building(s) from `{path:?}`");
        }
        sim.apply_decoded_entities(&save);
        if let Some(mut runtime) = sim.ecs.0.remove_resource::<CampaignRuntime>() {
            if let Some(rules) = save.rules.clone() {
                runtime.session.rules = rules;
            }
            if let Some(stats) = save.stats.clone() {
                runtime.session.stats = stats;
            }
            if let Some(wave) = save
                .tags
                .get("wave")
                .and_then(|value| value.parse::<i32>().ok())
            {
                runtime.session.wave = wave;
            }
            if let Some(wavetime) = save
                .tags
                .get("wavetime")
                .and_then(|value| value.parse::<f32>().ok())
            {
                runtime.session.wavetime = wavetime;
            }
            runtime.session.markers = save.markers.clone();
            apply_team_plans(&mut runtime.session.teams, save.plans.clone());
            runtime.session.phase = crate::game::State::Playing;
            sim.ecs.0.insert_resource(runtime);
        }
        // `WorldLoadEvent` fog half (FogControl.java:46-62): size the fog to
        // the just-swapped grid with the loaded rules.
        sim.reset_fog();
        // `static-fog-data` (`FogControl.read`): the custom chunk overwrites the
        // freshly reset fog, restoring each saved team's exploration.
        if let Some(bytes) = save.static_fog.as_ref()
            && let Some(mut fog) = sim.ecs.0.get_resource_mut::<crate::game::fog::FogControl>()
        {
            fog.read(bytes);
        }
        IoStatus::Ok
    }
}

impl IoHandler for SimIoHandler {
    fn apply(&mut self, sim: &mut Sim, requests: &[IoRequest]) -> Vec<IoResponse> {
        requests
            .iter()
            .map(|request| {
                let status = match request {
                    IoRequest::Load { path } => self.load(sim, path),
                    IoRequest::Save { .. } => IoStatus::Deferred,
                };
                IoResponse {
                    request: request.clone(),
                    status,
                }
            })
            .collect()
    }

    fn capture(&mut self, sim: &mut Sim, requests: &[IoRequest]) -> Vec<IoResponse> {
        requests
            .iter()
            .map(|request| {
                let status = match request {
                    IoRequest::Save { path, as_map } => self.save(sim, path, *as_map),
                    IoRequest::Load { .. } => IoStatus::Deferred,
                };
                IoResponse {
                    request: request.clone(),
                    status,
                }
            })
            .collect()
    }
}

/// Boots one base content registry (`createBaseContent` + `init` + `postInit`).
fn boot_registry() -> Result<ContentRegistry, ContentError> {
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut content = create_base_content(&bundle, &store, true)?;
    content.init()?;
    content.post_init()?;
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::content::BlockId;
    use crate::world::TilePos;

    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("mind-sim-io-{tag}-{}.msav", std::process::id()))
    }

    #[test]
    fn live_sim_save_then_load_roundtrips_the_tile_grid() {
        let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
        let wall = sim.content().id("stone-wall").expect("stone-wall");
        sim.apply(Command::Place {
            x: 2,
            y: 3,
            block: wall,
        })
        .expect("place");
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("roundtrip");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one save response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok);
        assert!(path.is_file(), "save file written");

        // Wipe the tile, then load it back at the next Apply boundary.
        sim.grid.fill(BlockId::AIR, BlockId::AIR);
        assert_eq!(sim.grid.block_at(TilePos::new(2, 3)), Some(BlockId::AIR));
        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one load response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok);
        assert_eq!(sim.grid.block_at(TilePos::new(2, 3)), Some(wall));

        let _ = std::fs::remove_file(&path);
    }

    /// A live-runtime building (plan 07 `Building` + modules) must write a
    /// decodable tile-entity chunk, so a materialized map/sector core survives
    /// the auto-save round trip (EV-0037).
    #[test]
    fn live_runtime_core_save_then_load_roundtrips() {
        let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("runtime");
        let core = sim.content().id("core-shard").expect("core-shard");
        sim.apply(Command::Place {
            x: 5,
            y: 5,
            block: core,
        })
        .expect("place core");
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("live-core-roundtrip");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one save response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok);
        assert!(path.is_file(), "save file written");

        sim.grid.fill(BlockId::AIR, BlockId::AIR);
        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one load response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");
        assert_eq!(sim.grid.block_at(TilePos::new(5, 5)), Some(core));

        let _ = std::fs::remove_file(&path);
    }

    /// PAR-0161: a live save round-trips the per-kind bodies — the drill keeps
    /// its progress/warmup and the sorter keeps its configured item. Before the
    /// fix both were dropped by the v1 `read_building` (progress 0, config None).
    #[test]
    fn live_save_restores_drill_and_sorter_bodies() {
        fn configure(sim: &mut Sim, x: i16, y: i16, value: crate::determinism::ConfigValue) {
            sim.command(crate::determinism::SimCommand::Configure { x, y, value })
                .expect("configure");
        }

        let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        let drill = sim
            .content()
            .id("mechanical-drill")
            .expect("mechanical-drill");
        let sorter = sim.content().id("sorter").expect("sorter");
        sim.apply(Command::Place {
            x: 4,
            y: 4,
            block: drill,
        })
        .expect("place drill");
        sim.apply(Command::Place {
            x: 8,
            y: 8,
            block: sorter,
        })
        .expect("place sorter");
        let drill_entity = sim.grid.tile(4, 4).build.expect("drill entity");
        {
            let mut state = sim
                .ecs
                .0
                .get_mut::<crate::entities::comp::DrillState>(drill_entity)
                .expect("drill state");
            state.progress = 123.5;
            state.warmup = 0.75;
        }
        configure(
            &mut sim,
            8,
            8,
            crate::determinism::ConfigValue::Content("copper".to_owned()),
        );
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("drill-sorter-bodies");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one save response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok);

        // Wipe the live world; the load must rebuild the per-kind bodies.
        sim.grid.fill(BlockId::AIR, BlockId::AIR);
        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one load response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

        let drill_entity = sim.grid.tile(4, 4).build.expect("drill respawned");
        let state = sim
            .ecs
            .0
            .get::<crate::entities::comp::DrillState>(drill_entity)
            .expect("drill state");
        assert!(
            (state.progress - 123.5).abs() < 1e-6,
            "drill progress {}",
            state.progress
        );
        assert!(
            (state.warmup - 0.75).abs() < 1e-6,
            "drill warmup {}",
            state.warmup
        );

        let sorter_entity = sim.grid.tile(8, 8).build.expect("sorter respawned");
        match crate::world::config::read_config(&sim.ecs.0, sorter_entity) {
            crate::world::config::ConfigValue::Item(item) => {
                assert_eq!(item, crate::content::ItemId::COPPER, "sorter keeps copper");
            }
            other => panic!("sorter config lost: {other:?}"),
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_of_a_missing_file_is_a_failed_response() {
        let mut sim = Sim::new(1, 4, 4, BlockId::AIR, BlockId::AIR);
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));
        let path = temp_path("missing");
        let _ = std::fs::remove_file(&path);
        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1);
        assert!(matches!(responses[0].status, IoStatus::Failed(_)));
    }

    /// T2: Java writes `tick` as a float (`SaveVersion.java:135`); the parser
    /// must floor it instead of silently keeping the old tick.
    #[test]
    fn meta_tick_parses_java_float() {
        let mut tags = StringMap::new();
        tags.insert("tick".to_owned(), "74802.1339981854".to_owned());
        assert_eq!(meta_tick(&tags), Some(74802));
        tags.insert("tick".to_owned(), "12".to_owned());
        assert_eq!(meta_tick(&tags), Some(12));
        tags.insert("tick".to_owned(), "x".to_owned());
        assert_eq!(meta_tick(&tags), None);
    }

    /// T2: the shared decoder carries the saved units and meta tags without
    /// touching the live sim (`SaveIO.load`'s world half) — the sector-resume
    /// path uses it to restore what the old `load_grid_file` dropped.
    #[test]
    fn decode_save_bytes_carries_units_tags_and_markers() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;

        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("content");
        let campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        let mut session = PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            unit_cap: 100,
            unit_cap_variable: false,
            ..Rules::default()
        });
        session.wavetime = 1234.5;

        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        sim.ecs
            .0
            .insert_resource(CampaignRuntime::new(session, campaign));
        sim.spawn_live_unit("dagger", 1, 40.0, 40.0, 0.0)
            .expect("spawn 1");
        sim.spawn_live_unit("dagger", 1, 48.0, 40.0, 0.0)
            .expect("spawn 2");
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("decode-units");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one save response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok);

        let bytes = std::fs::read(&path).expect("save bytes");
        let mut content = boot_registry().expect("content");
        let save = decode_save_bytes(&bytes, &mut content).expect("decode");
        assert_eq!(save.entities.units.len(), 2, "two units decoded");
        assert_eq!(
            save.tags.get("wavetime").map(String::as_str),
            Some("1234.5"),
            "wavetime tag carried"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// T9: revision 3 saves the real `spawnedByCore` flag, so a default-team
    /// core-type unit WITHOUT the flag (a factory-made alpha) survives the
    /// respawn; `UnitComp.java:868-871` only removes the flagged units (the
    /// host spawns the player's own unit). The v1/v2 name fallback is covered
    /// by `apply_decoded_entities_skips_spawned_by_core`.
    #[test]
    fn apply_decoded_entities_skips_core_units() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;

        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("content");
        let campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        let session = PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            unit_cap: 100,
            unit_cap_variable: false,
            ..Rules::default()
        });
        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        sim.ecs
            .0
            .insert_resource(CampaignRuntime::new(session, campaign));
        let alpha = sim
            .spawn_live_unit("alpha", 1, 100.0, 100.0, 0.0)
            .expect("alpha");
        let _ = alpha;
        sim.spawn_live_unit("dagger", 1, 120.0, 100.0, 0.0)
            .expect("dagger");
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("decode-core-unit");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses[0].status, IoStatus::Ok);

        let bytes = std::fs::read(&path).expect("save bytes");
        let mut content = boot_registry().expect("content");
        let save = decode_save_bytes(&bytes, &mut content).expect("decode");
        assert_eq!(save.entities.units.len(), 2, "both units decoded");

        // Apply to a fresh sim: both respawn (no `spawnedByCore` flag).
        let mut dst = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        dst.install_block_runtime().expect("block runtime");
        dst.install_unit_runtime().expect("unit runtime");
        dst.ecs.0.insert_resource(CampaignRuntime::new(
            PlaySession::new(Rules {
                default_team: 1,
                wave_team: 2,
                unit_cap: 100,
                unit_cap_variable: false,
                ..Rules::default()
            }),
            Campaign::from_registry(&registry, &EmptyNeighborhood),
        ));
        dst.apply_decoded_entities(&save);
        let runtime = dst.unit_runtime().expect("runtime");
        let mut names = Vec::new();
        for entity in runtime.units.iter().copied() {
            if let Some(comp) = dst.ecs.0.get::<UnitTypeComp>(entity)
                && let Some(def) = runtime.content.unit(comp.type_id)
            {
                names.push(def.name.clone());
            }
        }
        assert_eq!(names, vec!["alpha".to_owned(), "dagger".to_owned()]);

        let _ = std::fs::remove_file(&path);
    }

    /// T9: `spawnedByCore` default-team units are skipped on respawn; v1/v2
    /// saves (no flag) fall back to the core unit type names
    /// (`UnitComp.java:868-871`).
    #[test]
    fn apply_decoded_entities_skips_spawned_by_core() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;

        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("content");
        let unit = |name: &str, revision: u16, spawned_by_core: bool| DecodedUnit {
            revision,
            saved_id: -1,
            name: name.to_owned(),
            team: 1,
            x: 100.0,
            y: 100.0,
            rotation: 0.0,
            health: 150.0,
            payloads: Vec::new(),
            spawned_by_core,
            elevation: 0.0,
            shield: 0.0,
            stack: None,
            statuses: Vec::new(),
            command: None,
            controller: None,
            plans: Vec::new(),
        };
        let save_with = |units: Vec<DecodedUnit>| DecodedSave {
            grid: WorldGrid::new(0, 0),
            pending: Vec::new(),
            plans: Vec::new(),
            tags: StringMap::default(),
            rules: None,
            stats: None,
            markers: MapMarkers::default(),
            entities: LiveEntitySink {
                units,
                fires: Vec::new(),
                puddles: Vec::new(),
            },
            static_fog: None,
        };
        let mut dst = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        dst.install_block_runtime().expect("block runtime");
        dst.install_unit_runtime().expect("unit runtime");
        dst.ecs.0.insert_resource(CampaignRuntime::new(
            PlaySession::new(Rules {
                default_team: 1,
                wave_team: 2,
                unit_cap: 100,
                unit_cap_variable: false,
                ..Rules::default()
            }),
            Campaign::from_registry(&registry, &EmptyNeighborhood),
        ));
        // Revision 3: the flag skips the alpha; the dagger stays.
        dst.apply_decoded_entities(&save_with(vec![
            unit("alpha", 3, true),
            unit("dagger", 3, false),
        ]));
        let names: Vec<String> = dst
            .unit_runtime()
            .expect("runtime")
            .units
            .iter()
            .copied()
            .filter_map(|entity| {
                let comp = dst.ecs.0.get::<UnitTypeComp>(entity)?;
                dst.unit_runtime()?
                    .content
                    .unit(comp.type_id)
                    .map(|def| def.name.clone())
            })
            .collect();
        assert_eq!(names, vec!["dagger".to_owned()]);

        // Revision 2 (no flag): the core type name skips the alpha.
        let mut legacy = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        legacy.install_block_runtime().expect("block runtime");
        legacy.install_unit_runtime().expect("unit runtime");
        legacy.ecs.0.insert_resource(CampaignRuntime::new(
            PlaySession::new(Rules {
                default_team: 1,
                wave_team: 2,
                unit_cap: 100,
                unit_cap_variable: false,
                ..Rules::default()
            }),
            Campaign::from_registry(&registry, &EmptyNeighborhood),
        ));
        legacy.apply_decoded_entities(&save_with(vec![
            unit("alpha", 2, false),
            unit("dagger", 2, false),
        ]));
        let names: Vec<String> = legacy
            .unit_runtime()
            .expect("runtime")
            .units
            .iter()
            .copied()
            .filter_map(|entity| {
                let comp = legacy.ecs.0.get::<UnitTypeComp>(entity)?;
                legacy
                    .unit_runtime()?
                    .content
                    .unit(comp.type_id)
                    .map(|def| def.name.clone())
            })
            .collect();
        assert_eq!(names, vec!["dagger".to_owned()]);
    }

    /// H04 T13: `UnitComp.afterRead` (`UnitComp.java:585-593`). A world load
    /// spawns the saved units before the campaign session exists, so the spawn
    /// fallback (`BuildRules.defaultTeam` = derelict) gives a player-team unit
    /// the per-type AI. Once the session rules are installed,
    /// `reset_loaded_controllers` re-derives `type.createController`
    /// (`UnitType.java:281-283`): team `defaultTeam` -> `CommandAI`, wave team
    /// -> `GroundAI`; a saved `Player` bridge is never replaced.
    #[test]
    fn loaded_player_team_units_get_command_ai() {
        use crate::ai::controller::{AiKind, ControllerSlot};
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;

        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("content");
        let unit = |name: &str, team: u8, controller: Option<AiKind>| DecodedUnit {
            revision: 3,
            saved_id: -1,
            name: name.to_owned(),
            team,
            x: 100.0,
            y: 100.0,
            rotation: 0.0,
            health: 150.0,
            payloads: Vec::new(),
            spawned_by_core: false,
            elevation: 0.0,
            shield: 0.0,
            stack: None,
            statuses: Vec::new(),
            command: None,
            controller,
            plans: Vec::new(),
        };
        let save_with = |units: Vec<DecodedUnit>| DecodedSave {
            grid: WorldGrid::new(0, 0),
            pending: Vec::new(),
            plans: Vec::new(),
            tags: StringMap::default(),
            rules: None,
            stats: None,
            markers: MapMarkers::default(),
            entities: LiveEntitySink {
                units,
                fires: Vec::new(),
                puddles: Vec::new(),
            },
            static_fog: None,
        };
        let by_team = |sim: &Sim| -> Vec<(u8, AiKind)> {
            sim.unit_runtime()
                .expect("runtime")
                .units
                .iter()
                .copied()
                .filter_map(|entity| {
                    let team = sim.ecs.0.get::<TeamComp>(entity)?.team;
                    let kind = sim.ecs.0.get::<ControllerSlot>(entity)?.kind;
                    Some((team, kind))
                })
                .collect()
        };

        // World-load order: no `CampaignRuntime` yet (`load_sector` runs before
        // `start_sector_with_loadout` installs the session).
        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        sim.apply_decoded_entities(&save_with(vec![
            unit("dagger", 1, None),
            unit("dagger", 2, None),
            unit("dagger", 1, Some(AiKind::Player)),
        ]));
        // Fallback: every team but derelict reads as AI, so both daggers get
        // their per-type `GroundAI` (the bug state).
        assert!(by_team(&sim).iter().enumerate().all(|(i, (_, kind))| {
            if i == 2 {
                *kind == AiKind::Player
            } else {
                *kind == AiKind::Ground
            }
        }));

        // The session rules reach the world; Java's `afterRead` then re-derives
        // the controllers. A live campaign sector (`PlaySession.sector`) is
        // what makes wave team 2 an AI team (`Team.isAI`) even with
        // `rules.waves` false.
        let mut session = PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            unit_cap: 100,
            unit_cap_variable: false,
            ..Rules::default()
        });
        session.sector = Some((crate::content::PlanetId::new(0), 170));
        sim.ecs.0.insert_resource(CampaignRuntime::new(
            session,
            Campaign::from_registry(&registry, &EmptyNeighborhood),
        ));
        sim.reset_loaded_controllers();
        let kinds = by_team(&sim);
        assert!(
            kinds.contains(&(1, AiKind::Command)),
            "player team: {kinds:?}"
        );
        assert!(kinds.contains(&(2, AiKind::Ground)), "wave team: {kinds:?}");
        assert!(
            kinds.contains(&(1, AiKind::Player)),
            "player bridge: {kinds:?}"
        );

        // The stash is consumed: a second call changes nothing.
        sim.reset_loaded_controllers();
        assert_eq!(by_team(&sim), kinds);
    }

    /// H04 T13: a revision-3 unit whose saved controller keeps state
    /// (`LogicAI`, `AIController.keepState`) survives `afterRead`.
    #[test]
    fn loaded_logic_unit_keeps_state() {
        use crate::ai::controller::{AiKind, ControllerSlot};
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;

        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("content");
        let save = DecodedSave {
            grid: WorldGrid::new(0, 0),
            pending: Vec::new(),
            plans: Vec::new(),
            tags: StringMap::default(),
            rules: None,
            stats: None,
            markers: MapMarkers::default(),
            entities: LiveEntitySink {
                units: vec![DecodedUnit {
                    revision: 3,
                    saved_id: -1,
                    name: "dagger".to_owned(),
                    team: 1,
                    x: 100.0,
                    y: 100.0,
                    rotation: 0.0,
                    health: 150.0,
                    payloads: Vec::new(),
                    spawned_by_core: false,
                    elevation: 0.0,
                    shield: 0.0,
                    stack: None,
                    statuses: Vec::new(),
                    command: None,
                    controller: Some(AiKind::Logic),
                    plans: Vec::new(),
                }],
                fires: Vec::new(),
                puddles: Vec::new(),
            },
            static_fog: None,
        };

        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        sim.apply_decoded_entities(&save);
        sim.ecs.0.insert_resource(CampaignRuntime::new(
            PlaySession::new(Rules {
                default_team: 1,
                wave_team: 2,
                unit_cap: 100,
                unit_cap_variable: false,
                ..Rules::default()
            }),
            Campaign::from_registry(&registry, &EmptyNeighborhood),
        ));
        sim.reset_loaded_controllers();
        let entity = sim.unit_runtime().expect("runtime").units[0];
        assert_eq!(
            sim.ecs
                .0
                .get::<ControllerSlot>(entity)
                .map(|slot| slot.kind),
            Some(AiKind::Logic)
        );
    }

    /// H04 T10: the reference client's Ground Zero save (v13 `MSAV`) entity
    /// region decodes — the only chunk is the player's core alpha
    /// (`spawnedByCore`), which `apply_decoded_entities` then skips
    /// (`apply_decoded_entities_skips_core_units` contract: 0 units restored).
    #[cfg(feature = "msav-import")]
    #[test]
    fn java_gz_fixture_skips_core_unit() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;

        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        registry.init().expect("init");
        registry.post_init().expect("post init");
        let path = format!(
            "{}/tests/fixtures/saves/java-gz-r05.msav",
            env!("CARGO_MANIFEST_DIR")
        );
        let bytes = std::fs::read(&path).expect("java gz fixture");
        let save = decode_save_bytes(&bytes, &mut registry).expect("decode");
        assert_eq!(save.entities.units.len(), 1, "the core alpha decodes");
        assert_eq!(save.entities.units[0].name, "alpha");
        assert!(save.entities.units[0].spawned_by_core);
        assert_eq!(save.entities.units[0].team, 1);

        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        sim.ecs.0.insert_resource(CampaignRuntime::new(
            PlaySession::new(Rules {
                default_team: 1,
                wave_team: 2,
                unit_cap: 100,
                unit_cap_variable: false,
                ..Rules::default()
            }),
            Campaign::from_registry(&registry, &EmptyNeighborhood),
        ));
        sim.apply_decoded_entities(&save);
        assert!(
            sim.unit_runtime().expect("runtime").units.is_empty(),
            "the core unit is skipped on apply"
        );
    }

    /// The live unit/fire/puddle chunk class ids must stay clear of the
    /// upstream `classids.properties` table (append-only ABI).
    #[test]
    fn live_unit_class_id_is_reserved() {
        let mapping = crate::entities::mapping::EntityMapping::load_default();
        for id in [SIM_UNIT_CLASS_ID, SIM_FIRE_CLASS_ID, SIM_PUDDLE_CLASS_ID] {
            assert!(
                mapping.name_of(u16::from(id)).is_none(),
                "class id {id} collides with an upstream class id"
            );
        }
        assert_eq!(SIM_UNIT_REVISION, 3);
        assert_eq!(SIM_FIRE_REVISION, 1);
        assert_eq!(SIM_PUDDLE_REVISION, 1);
    }

    /// H04 T9: `entity_count` equals the chunks `write_entities` emits even
    /// when a unit is filtered out; the reader then never eats the fire bytes
    /// as a unit chunk (`SaveVersion.java:425-439`).
    #[test]
    fn entity_count_matches_written_chunks() {
        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let first = sim
            .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
            .expect("dagger 1");
        sim.spawn_live_unit("dagger", 2, 120.0, 100.0, 0.0)
            .expect("dagger 2");
        // Filtered out of the chunk stream: no `UnitCore`.
        sim.ecs.0.entity_mut(first).remove::<UnitCore>();
        sim.ecs.0.spawn((
            Pos { x: 104.0, y: 104.0 },
            crate::combat::fires::FireState {
                tile: (13, 13),
                time: 5.0,
                lifetime: 600.0,
                puddle_flammability: 0.0,
                spread_timer: 0.0,
                fireball_timer: 0.0,
                damage_timer: 0.0,
                warmup: 0.0,
                animation: 0.0,
            },
        ));
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("entity-count");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses[0].status, IoStatus::Ok);

        let bytes = std::fs::read(&path).expect("save bytes");
        let mut content = boot_registry().expect("content");
        let save = decode_save_bytes(&bytes, &mut content).expect("decode");
        assert_eq!(save.entities.units.len(), 1, "only the live dagger");
        assert_eq!(save.entities.fires.len(), 1, "the fire chunk follows");

        let mut dst = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        dst.install_block_runtime().expect("block runtime");
        dst.install_unit_runtime().expect("unit runtime");
        dst.apply_decoded_entities(&save);
        assert_eq!(
            dst.unit_runtime().expect("runtime").units.len(),
            1,
            "one dagger restored"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// H04 T9: revision 3 round-trips elevation, the carried item, statuses,
    /// shield, the command id + move target and the builder plan queue.
    #[test]
    fn unit_rev3_round_trip() {
        use crate::ai::controller::UnitCommandComp;
        use crate::entities::comp::unit::comp::{ItemsComp, ShieldComp, StatusComp, StatusEntry};
        use crate::sim::builder::{BuilderPlans, UnitBuildPlan};

        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let entity = sim
            .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
            .expect("dagger");
        let content = boot_registry().expect("content");
        let copper = content.item_id("copper").expect("copper");
        let wet = content.status_id("wet").expect("wet");
        if let Some(mut core) = sim.ecs.0.get_mut::<UnitCore>(entity) {
            core.elevation = 0.5;
        }
        sim.ecs.0.entity_mut(entity).insert(ItemsComp {
            item: Some((copper, 10)),
        });
        sim.ecs.0.entity_mut(entity).insert(ShieldComp {
            shield: 20.0,
            ..Default::default()
        });
        sim.ecs.0.entity_mut(entity).insert(StatusComp {
            statuses: vec![StatusEntry::new(wet, 120.0)],
            damage_times: vec![0.0],
            ..Default::default()
        });
        sim.ecs.0.entity_mut(entity).insert(UnitCommandComp(3));
        if let Some(mut slot) = sim
            .ecs
            .0
            .get_mut::<crate::ai::controller::ControllerSlot>(entity)
        {
            slot.kind = crate::ai::controller::AiKind::Logic;
        } else {
            sim.ecs
                .0
                .entity_mut(entity)
                .insert(crate::ai::controller::ControllerSlot::new(
                    crate::ai::controller::AiKind::Logic,
                ));
        }
        sim.ecs.0.entity_mut(entity).insert(BuilderPlans {
            plans: std::collections::VecDeque::from([UnitBuildPlan::place(
                10,
                12,
                1,
                BlockId::STONE_WALL,
            )]),
            ..Default::default()
        });
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("unit-rev3");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses[0].status, IoStatus::Ok);

        let bytes = std::fs::read(&path).expect("save bytes");
        let mut content = boot_registry().expect("content");
        let save = decode_save_bytes(&bytes, &mut content).expect("decode");
        assert_eq!(save.entities.units.len(), 1);
        let unit = &save.entities.units[0];
        assert_eq!(unit.revision, SIM_UNIT_REVISION);
        assert_eq!(unit.elevation, 0.5);
        assert_eq!(unit.stack, Some((copper, 10)));
        assert_eq!(unit.shield, 20.0);
        assert_eq!(unit.statuses.len(), 1);
        assert_eq!(unit.statuses[0].effect, wet);
        assert_eq!(
            unit.statuses[0].duration, 119.0,
            "one capture tick elapsed before the save"
        );
        assert_eq!(unit.command, Some(3));
        assert_eq!(unit.controller, Some(crate::ai::controller::AiKind::Logic));
        assert_eq!(unit.plans.len(), 1);
        assert_eq!(unit.plans[0].block, BlockId::STONE_WALL);
        assert_eq!((unit.plans[0].x, unit.plans[0].y), (10, 12));

        let _ = std::fs::remove_file(&path);
    }

    /// A live save must carry real meta (`tick`/`playtime`) and the entities
    /// region must round-trip the placed building and the live unit: load
    /// materializes the building into the ECS runtime and respawns the unit.
    #[test]
    fn live_sim_save_then_load_restores_entities_and_meta() {
        let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let core = sim.content().id("core-shard").expect("core-shard");
        sim.apply(Command::Place {
            x: 5,
            y: 5,
            block: core,
        })
        .expect("place core");
        for _ in 0..5 {
            sim.tick().expect("tick");
        }
        sim.spawn_live_unit("dagger", 1, 64.0, 64.0, 0.0)
            .expect("spawn unit");
        sim.set_save_playtime(4321);
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("live-entities-roundtrip");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one save response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok);

        let meta = SaveIo::get_meta(&NativeFs, &path).expect("meta");
        assert_eq!(meta.time_played, 4321, "host playtime lands in the meta");
        let saved_tick: u64 = meta
            .tags
            .get("tick")
            .and_then(|tick| tick.parse().ok())
            .expect("tick tag");
        assert!(saved_tick > 0, "tick tag is live: {saved_tick}");

        // Wipe the live world; the load must rebuild it from the file.
        sim.grid.fill(BlockId::AIR, BlockId::AIR);
        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one load response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");
        assert_eq!(sim.grid.block_at(TilePos::new(5, 5)), Some(core));
        assert_eq!(sim.group_counts().get("build"), Some(&1));
        assert_eq!(sim.group_counts().get("unit"), Some(&1));
        assert!(sim.tick_count() >= saved_tick, "tick header resumes");

        let _ = std::fs::remove_file(&path);
    }

    /// PAR-0249: a core holding `i32::MAX / 2` item stacks (the `fillItems`
    /// value under `coreIncinerates`, `CoreBlock.java:749-751`) saves and
    /// reloads without panicking; `ItemModule.total` wraps like Java's `int`
    /// (`ItemModule.java:319-334`).
    #[test]
    fn live_sim_save_then_load_keeps_oversized_item_stacks() {
        use crate::world::modules::ItemModule;

        let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let core = sim.content().id("core-shard").expect("core-shard");
        sim.apply(Command::Place {
            x: 5,
            y: 5,
            block: core,
        })
        .expect("place core");
        for _ in 0..5 {
            sim.tick().expect("tick");
        }
        let entity = sim.grid.entity_at(TilePos::new(5, 5)).expect("core entity");
        let content = boot_registry().expect("content");
        let copper = content.item_id("copper").expect("copper");
        let lead = content.item_id("lead").expect("lead");
        let graphite = content.item_id("graphite").expect("graphite");
        let silicon = content.item_id("silicon").expect("silicon");
        let huge = i32::MAX / 2;
        // Four `i32::MAX / 2` stacks sum past `i32::MAX`: the plain `+=` the
        // decoder used panicked here in debug builds (the H03 crash report).
        let expected_total = huge
            .wrapping_add(huge)
            .wrapping_add(huge)
            .wrapping_add(huge);
        {
            let mut items = sim.ecs.0.get_mut::<ItemModule>(entity).expect("items");
            items.set(copper, huge);
            items.set(lead, huge);
            items.set(graphite, huge);
            items.set(silicon, huge);
            assert_eq!(items.total(), expected_total, "Java int wrap (set)");
        }
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("live-oversized-items-roundtrip");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one save response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok);

        // Wipe the live world; the load rebuilds the core and must not panic
        // on the overflow the sum of the two stacks produces in `apply_base`.
        sim.grid.fill(BlockId::AIR, BlockId::AIR);
        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one load response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

        let entity = sim
            .grid
            .entity_at(TilePos::new(5, 5))
            .expect("core reloaded");
        let items = sim.ecs.0.get::<ItemModule>(entity).expect("items");
        assert_eq!(items.get(copper), huge, "stack round-trips");
        assert_eq!(items.get(lead), huge, "stack round-trips");
        assert_eq!(items.get(graphite), huge, "stack round-trips");
        assert_eq!(items.get(silicon), huge, "stack round-trips");
        assert_eq!(items.total(), expected_total, "wrapped total round-trips");
        // The live module keeps working after the oversized load.
        let mut items = sim.ecs.0.get_mut::<ItemModule>(entity).expect("items");
        assert_eq!(items.remove(copper, 1), 1);
        assert_eq!(items.total(), expected_total.wrapping_sub(1));

        let _ = std::fs::remove_file(&path);
    }

    /// H09 T11: the `static-fog-data` custom chunk (`FogControl.java:97,
    /// 437-505`) round-trips a live save: the reloaded fog has the same
    /// discovered tiles as the saved one.
    #[test]
    fn live_save_keeps_static_fog() {
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;

        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let campaign = {
            let content = &sim.unit_runtime().expect("rt").content;
            Campaign::from_registry(content, &EmptyNeighborhood)
        };
        let rules = Rules {
            default_team: 1,
            waves: true,
            fog: true,
            static_fog: true,
            ..Rules::default()
        };
        sim.ecs
            .0
            .insert_resource(CampaignRuntime::new(PlaySession::new(rules), campaign));
        sim.reset_fog();
        let _dagger = sim
            .spawn_live_unit("dagger", 1, 20.0 * 8.0 + 4.0, 20.0 * 8.0 + 4.0, 0.0)
            .expect("dagger");
        for _ in 0..4 {
            sim.tick().expect("tick");
        }
        let sample: Vec<(i32, i32)> = (18..23)
            .flat_map(|y| (18..23).map(move |x| (x, y)))
            .collect();
        let before: Vec<bool> = sample
            .iter()
            .map(|(x, y)| sim.fog_discovered(1, *x, *y))
            .collect();
        assert!(
            before.iter().any(|discovered| *discovered),
            "the unit discovered tiles"
        );
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("live-static-fog");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one save response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok);

        // Wipe the live world; the load must restore the fog from the chunk.
        sim.grid.fill(BlockId::AIR, BlockId::AIR);
        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one load response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");
        for (index, (x, y)) in sample.iter().enumerate() {
            assert_eq!(
                sim.fog_discovered(1, *x, *y),
                before[index],
                "discovered ({x}, {y}) round-trips"
            );
        }

        let _ = std::fs::remove_file(&path);
    }

    /// PAR-0138: a live save round-trips the carried payload stack (revision 2
    /// unit chunk); a v1 save without payloads still loads.
    #[test]
    fn live_save_keeps_unit_payloads() {
        use crate::entities::comp::unit::comp::{CarriedPayload, UnitPayloads};

        let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let mega = sim
            .spawn_player_unit("mega", 1, 100.0, 100.0, 0.0)
            .expect("mega");
        sim.spawn_live_unit("dagger", 1, 110.0, 100.0, 0.0)
            .expect("dagger");
        assert!(
            sim.apply_payload_command(0, 0.0, 0.0, None),
            "the dagger is picked up"
        );
        let before = sim
            .ecs
            .0
            .get::<UnitPayloads>(mega)
            .expect("payloads")
            .stack
            .clone();
        assert!(matches!(before.as_slice(), [CarriedPayload::Unit { .. }]));

        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));
        let path = temp_path("live-unit-payloads");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one save response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok);

        // Wipe the live world; the load must restore the carried stack.
        sim.grid.fill(BlockId::AIR, BlockId::AIR);
        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one load response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

        let mega_id = sim
            .unit_runtime()
            .expect("rt")
            .content
            .unit_id("mega")
            .expect("mega id");
        let loaded = sim
            .unit_runtime()
            .expect("rt")
            .units
            .iter()
            .copied()
            .find(|entity| {
                sim.ecs
                    .0
                    .get::<crate::entities::comp::unit::comp::UnitTypeComp>(*entity)
                    .is_some_and(|comp| comp.type_id == mega_id)
            })
            .expect("mega respawned");
        assert_eq!(
            sim.ecs
                .0
                .get::<UnitPayloads>(loaded)
                .map(|payloads| payloads.stack.clone()),
            Some(before),
            "the carried stack survived the round-trip"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// PAR-0090: a live save keeps fires and puddles and the load restores
    /// them with the same tile/amount/time.
    #[test]
    fn live_sim_save_keeps_fires_and_puddles() {
        let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let mut rng = crate::determinism::SimRng::new(5);
        let fire = crate::combat::fires::create(&mut sim.ecs.0, 7, 7, &mut rng).expect("fire");
        {
            let world = &mut sim.ecs.0;
            let mut runtime = world.remove_non_send::<UnitRuntime>().expect("runtime");
            let water = runtime.content.liquid_id("water").expect("water");
            let puddle = crate::combat::puddles::deposit(
                world,
                &sim.grid.tiles,
                &runtime.content,
                9,
                9,
                water,
                30.0,
                &mut runtime.rng,
            );
            assert!(puddle.is_some(), "puddle deposited");
            world.insert_non_send(runtime);
        }
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("live-fires-puddles-roundtrip");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one save response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok);
        let fire_before = sim
            .ecs
            .0
            .get::<crate::combat::fires::FireState>(fire)
            .copied()
            .expect("fire state");
        let puddle_before = sim
            .ecs
            .0
            .iter_entities()
            .find_map(|entity| entity.get::<crate::combat::puddles::PuddleState>().copied())
            .expect("puddle state");

        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one load response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

        let fires: Vec<crate::combat::fires::FireState> = sim
            .ecs
            .0
            .iter_entities()
            .filter_map(|entity| entity.get::<crate::combat::fires::FireState>().copied())
            .collect();
        assert_eq!(fires.len(), 1, "one fire restored");
        assert_eq!(fires[0].tile, fire_before.tile);
        // One tick (the load tick) advances the restored fire: 1 + the
        // bare-ground 8 (`FireComp.java:67-69`).
        assert!(
            (fires[0].time - fire_before.time - 9.0).abs() < 0.001,
            "fire time resumes ({} vs {})",
            fires[0].time,
            fire_before.time
        );
        let puddles: Vec<crate::combat::puddles::PuddleState> = sim
            .ecs
            .0
            .iter_entities()
            .filter_map(|entity| entity.get::<crate::combat::puddles::PuddleState>().copied())
            .collect();
        assert_eq!(puddles.len(), 1, "one puddle restored");
        assert_eq!(puddles[0].tile, puddle_before.tile);
        assert_eq!(puddles[0].liquid, puddle_before.liquid);
        assert!(
            puddles[0].amount <= puddle_before.amount,
            "puddle amount resumes ({} <= {})",
            puddles[0].amount,
            puddle_before.amount
        );

        let _ = std::fs::remove_file(&path);
    }

    /// I01 T10: the unit's `EntitySeq` (written as the chunk id,
    /// `SaveVersion.writeEntity`: `out.i(entity.id())`) survives a save/load;
    /// `unit_by_id` resolves it and the allocators move past the restored ids
    /// (`EntityGroup.checkNextId`).
    #[test]
    fn unit_ids_round_trip_through_save() {
        let sim_seq = |sim: &Sim, entity| sim.ecs.0.get::<EntitySeq>(entity).map(|seq| seq.0);
        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let first = sim
            .spawn_live_unit("dagger", 2, 40.0, 40.0, 0.0)
            .expect("first dagger");
        let second = sim
            .spawn_live_unit("flare", 2, 48.0, 40.0, 0.0)
            .expect("flare");
        let first_seq = sim_seq(&sim, first).expect("first seq");
        let second_seq = sim_seq(&sim, second).expect("second seq");
        assert_ne!(first_seq, second_seq, "distinct ids");
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("unit-ids");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one save response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok);

        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one load response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

        // Every restored unit kept its saved id, in saved order.
        let restored: Vec<(u64, String)> = sim
            .unit_runtime()
            .expect("runtime")
            .units
            .iter()
            .copied()
            .filter_map(|entity| {
                let seq = sim.ecs.0.get::<EntitySeq>(entity)?.0;
                let name = sim
                    .unit_runtime()?
                    .content
                    .unit(sim.ecs.0.get::<UnitTypeComp>(entity)?.type_id)?
                    .name
                    .clone();
                Some((seq, name))
            })
            .collect();
        assert_eq!(
            restored,
            vec![
                (first_seq, "dagger".to_owned()),
                (second_seq, "flare".to_owned())
            ],
            "ids and order survive"
        );
        assert_eq!(
            crate::sim::unit_runtime::unit_by_id(&sim.ecs.0, first_seq as i32)
                .and_then(|entity| sim_seq(&sim, entity)),
            Some(first_seq),
            "unit_by_id resolves the saved id"
        );

        // The allocators moved past the restored ids: a new spawn cannot reuse
        // either id (`EntityGroup.checkNextId`).
        let max = first_seq.max(second_seq);
        let third = sim
            .spawn_live_unit("dagger", 2, 56.0, 40.0, 0.0)
            .expect("third dagger");
        let third_seq = sim_seq(&sim, third).expect("third seq");
        assert!(third_seq > max, "allocator bumped ({third_seq} > {max})");

        let _ = std::fs::remove_file(&path);
    }

    /// I01 T10: `UnitCargoLoaderBuild`'s save body (the tethered unit id,
    /// `UnitCargoLoader.java:174-185`) re-tethers the unit after a load through
    /// `readUnitId` + `Groups.unit.getByID` (here `unit_by_id`).
    #[test]
    fn cargo_loader_relinks_unit_after_load() {
        let sim_seq = |sim: &Sim, entity| sim.ecs.0.get::<EntitySeq>(entity).map(|seq| seq.0);
        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        // Place on team 1: derelict (team 0) buildings are disabled by
        // `checkAllowUpdate` and would never reach `updateTile`.
        sim.ecs
            .0
            .get_resource_mut::<crate::world::limits::BuildRules>()
            .expect("build rules")
            .default_team = 1;
        let loader = sim
            .content()
            .id("unit-cargo-loader")
            .expect("unit-cargo-loader");
        sim.apply(Command::Place {
            x: 8,
            y: 8,
            block: loader,
        })
        .expect("place loader");
        let loader_entity = sim.grid.tile(8, 8).build.expect("loader entity");
        let manifold = sim
            .spawn_live_unit("manifold", 2, 64.0, 64.0, 90.0)
            .expect("manifold");
        let manifold_seq = sim_seq(&sim, manifold).expect("manifold seq");
        {
            let mut state = sim
                .ecs
                .0
                .get_mut::<crate::world::blocks::units::CargoLoaderState>(loader_entity)
                .expect("loader state");
            state.unit = Some(manifold);
        }
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("cargo-loader-relink");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses[0].status, IoStatus::Ok);

        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

        let loader_entity = sim.grid.tile(8, 8).build.expect("loader respawned");
        let state = sim
            .ecs
            .0
            .get::<crate::world::blocks::units::CargoLoaderState>(loader_entity)
            .expect("loader state");
        assert_eq!(
            state.read_unit_id, -1,
            "pending read id resolved on updateTile"
        );
        let relinked = state.unit.expect("unit relinked after load");
        assert_eq!(
            sim_seq(&sim, relinked),
            Some(manifold_seq),
            "relinked to the saved unit id"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// I01 T10: the cargo loader body stays stream-aligned with the following
    /// building — a save holding a loader and a drill restores BOTH bodies
    /// (H02 T13 recorded the unimplemented body as a "misreads the following
    /// building" blocker).
    #[test]
    fn save_with_cargo_loader_reads_next_building() {
        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        sim.ecs
            .0
            .get_resource_mut::<crate::world::limits::BuildRules>()
            .expect("build rules")
            .default_team = 1;
        let loader = sim
            .content()
            .id("unit-cargo-loader")
            .expect("unit-cargo-loader");
        let drill = sim
            .content()
            .id("mechanical-drill")
            .expect("mechanical-drill");
        sim.apply(Command::Place {
            x: 8,
            y: 8,
            block: loader,
        })
        .expect("place loader");
        sim.apply(Command::Place {
            x: 12,
            y: 12,
            block: drill,
        })
        .expect("place drill");
        let drill_entity = sim.grid.tile(12, 12).build.expect("drill entity");
        {
            let mut state = sim
                .ecs
                .0
                .get_mut::<crate::entities::comp::building::DrillState>(drill_entity)
                .expect("drill state");
            state.progress = 77.5;
        }
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("cargo-loader-next-building");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses[0].status, IoStatus::Ok);

        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses[0].status, IoStatus::Ok, "save reloads");

        let drill_entity = sim.grid.tile(12, 12).build.expect("drill respawned");
        let state = sim
            .ecs
            .0
            .get::<crate::entities::comp::building::DrillState>(drill_entity)
            .expect("drill state survived the loader body");
        assert!(
            (state.progress - 77.5).abs() < 1e-6,
            "drill body aligned after the loader body: {}",
            state.progress
        );

        let _ = std::fs::remove_file(&path);
    }
}
