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
/// per payload `u8 tag` + fields). v1 saves (no payloads) still load.
pub const SIM_UNIT_REVISION: u16 = 2;

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
struct DecodedUnit {
    /// Content unit name (`UnitType.name`).
    name: String,
    /// Team id.
    team: u8,
    /// World-pixel x.
    x: f32,
    /// World-pixel y.
    y: f32,
    /// Facing angle in degrees.
    rotation: f32,
    /// Current health.
    health: f32,
    /// Carried payload stack (revision 2+; empty in v1 saves).
    payloads: Vec<crate::entities::comp::unit::comp::CarriedPayload>,
}

/// One decoded live fire record (`FireComp.read` sim fields).
#[derive(Debug, Clone, Copy, PartialEq)]
struct DecodedFire {
    /// Owning tile.
    tile: (i16, i16),
    /// Ticks lived.
    time: f32,
    /// Ticks to live.
    lifetime: f32,
}

/// One decoded live puddle record (`PuddleComp.read` sim fields).
#[derive(Debug, Clone, Copy, PartialEq)]
struct DecodedPuddle {
    /// Owning tile.
    tile: (i16, i16),
    /// Liquid content id.
    liquid: crate::content::LiquidId,
    /// Current amount.
    amount: f32,
}

/// Read-side entity sink collecting the native live-unit/fire/puddle chunks.
#[derive(Default)]
struct LiveEntitySink {
    units: Vec<DecodedUnit>,
    fires: Vec<DecodedFire>,
    puddles: Vec<DecodedPuddle>,
}

impl EntitySink for LiveEntitySink {
    fn supports_class(&self, class_id: u8, _custom_name: Option<&str>) -> bool {
        matches!(
            class_id,
            SIM_UNIT_CLASS_ID | SIM_FIRE_CLASS_ID | SIM_PUDDLE_CLASS_ID
        )
    }

    fn read_entity(
        &mut self,
        class_id: u8,
        _custom_name: Option<&str>,
        _id: i32,
        r: &mut WireReader,
    ) -> IoResult<()> {
        match class_id {
            SIM_UNIT_CLASS_ID => {
                let revision = r.us()?;
                if revision != 1 && revision != SIM_UNIT_REVISION {
                    return Err(IoError::corrupt(format!(
                        "unsupported live unit revision {revision}"
                    )));
                }
                let mut decoded = DecodedUnit {
                    name: r.str()?,
                    team: r.ub()?,
                    x: r.f()?,
                    y: r.f()?,
                    rotation: r.f()?,
                    health: r.f()?,
                    payloads: Vec::new(),
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
            other => {
                return Err(IoError::corrupt(format!(
                    "unknown live entity class id {other}"
                )));
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
        let units = self
            .sim
            .unit_runtime()
            .map(|runtime| runtime.unit_count(&self.sim.ecs.0))
            .unwrap_or(0);
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

    fn write_entities(&self, w: &mut WireWriter, scratch: &mut SaveScratch) -> IoResult<()> {
        let world = &self.sim.ecs.0;
        let Some(runtime) = world.get_non_send::<UnitRuntime>() else {
            return Ok(());
        };
        let mut live: Vec<(u64, Entity)> = runtime
            .units
            .iter()
            .copied()
            .filter(|entity| world.get_entity(*entity).is_ok())
            .map(|entity| {
                (
                    world
                        .get::<EntitySeq>(entity)
                        .map(|seq| seq.0)
                        .unwrap_or(u64::MAX),
                    entity,
                )
            })
            .collect();
        live.sort_by_key(|(seq, entity)| (*seq, entity.index()));
        for (seq, entity) in live {
            let (Some(core), Some(unit), Some(pos)) = (
                world.get::<UnitCore>(entity),
                world.get::<UnitTypeComp>(entity),
                world.get::<Pos>(entity),
            ) else {
                continue;
            };
            let Some(name) = runtime
                .content
                .unit(unit.type_id)
                .map(|def| def.name.clone())
            else {
                continue;
            };
            let team = world.get::<TeamComp>(entity).map(|t| t.team).unwrap_or(0);
            let health = world.get::<Health>(entity).map(|h| h.health).unwrap_or(0.0);
            scratch.write_chunk(w, |chunk| {
                chunk.ub(SIM_UNIT_CLASS_ID);
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
    let (result, tags, rules, stats, plans, pending) = {
        let mut context = crate::editor::context::EditorContext::new(&mut grid, registry);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(registry),
            entities: Some(&mut entities),
            markers: Some(&mut markers),
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
    Ok(DecodedSave {
        grid,
        pending,
        plans,
        tags,
        rules,
        stats,
        markers,
        entities,
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
}

impl Sim {
    /// Respawns the decoded world entities (units, fires, puddles) into the
    /// live runtime (`SaveVersion.readWorldEntities` application half).
    pub fn apply_decoded_entities(&mut self, save: &DecodedSave) {
        let default_team = self
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .map(|runtime| runtime.session.rules.default_team)
            .unwrap_or_else(|| crate::game::rules::Rules::default().default_team);
        for unit in &save.entities.units {
            // `UnitComp.java:868-871` despawns a `spawnedByCore` unit that is
            // not the player's, and the host spawns a fresh player unit
            // (`spawn_player_unit`). Until revision 3 carries the flag (T9),
            // the core unit types on the default team are skipped by name
            // (`Blocks.java` `coreShard..coreAcropolis` `unitType`).
            const CORE_UNITS: [&str; 6] = ["alpha", "beta", "gamma", "evoke", "incite", "emanate"];
            if unit.team == default_team && CORE_UNITS.contains(&unit.name.as_str()) {
                continue;
            }
            if let Some(entity) =
                self.spawn_live_unit(&unit.name, unit.team, unit.x, unit.y, unit.rotation)
            {
                if unit.health > 0.0
                    && let Some(mut health) = self.ecs.0.get_mut::<Health>(entity)
                {
                    health.health = unit.health;
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
        let mut ctx = WriteContext::meta_only(tags);
        ctx.content = Some(&content);
        ctx.map = Some(&source);
        ctx.entities = Some(&entities);
        ctx.markers = markers;
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
        if let Some(mut unit_runtime) = sim.ecs.0.get_non_send_mut::<UnitRuntime>() {
            unit_runtime.reset(&grid);
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

    /// T2: a core unit (`spawnedByCore`, until T9's flag lands matched by type
    /// name) is skipped on respawn — the host spawns the player unit itself;
    /// the other unit survives.
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
        sim.spawn_live_unit("alpha", 1, 100.0, 100.0, 0.0)
            .expect("alpha");
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

        // Apply to a fresh sim: only the dagger respawns.
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
        assert_eq!(names, vec!["dagger".to_owned()], "core unit skipped");

        let _ = std::fs::remove_file(&path);
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
        assert_eq!(SIM_UNIT_REVISION, 2);
        assert_eq!(SIM_FIRE_REVISION, 1);
        assert_eq!(SIM_PUDDLE_REVISION, 1);
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
        // One tick (the load tick) advances the restored fire/puddle.
        assert!(
            (fires[0].time - fire_before.time - 1.0).abs() < 0.001,
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
}
