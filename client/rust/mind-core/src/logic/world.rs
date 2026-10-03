// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Privileged world-instruction state and sinks (plan 13 M6).
//!
//! Ported from the world/privileged half of `core/src/mindustry/logic/LExecutor.java`
//! (`SetBlockI`, `SpawnUnitI`, `SpawnBulletI`, `SenseWeatherI`/`SetWeatherI`,
//! `ApplyEffectI`, `SetRuleI`, `FlushMessageI`, `CutsceneI`, `EffectI`,
//! `ExplosionI`, `SyncI`, `ClientDataI`, `GetFlagI`/`SetFlagI`, `SpawnWaveI`,
//! `PlaySoundI`, `PlayMusicI`, `SetMarkerI`, `MakeMarkerI`, `LocalePrintI`,
//! `QueryI`, `FetchI`).
//!
//! Plan 12's real [`Rules`], [`MapMarkers`] and `objective_flags` are now on
//! disk; this module owns the *world* half the VM needs without a `PlaySession`
//! borrow: [`LogicWorldState`] is a bevy `Resource` installed by the harness/host
//! that carries the mutable rules/markers plus a deterministic event log. Sinks
//! that require `ContentRegistry`/plan 10-11-17 harnesses (unit/bullet spawn,
//! status, FX, audio) are recorded as typed [`LogicWorldEvent`]s and host-gated,
//! matching the D2 relay/command design (`HIGH_LEVEL_PLAN` §2.2, plan 13 §8 R10);
//! the owning harness applies them. `MessageState` is the deviation-5 seam.

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::query::With;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::content::{BlockId, UnitTypeId};
use crate::content::{ContentRef, ContentType};
use crate::entities::comp::unit::{HitboxComp, UnitTypeComp};
use crate::entities::comp::{Building, Pos, TeamComp, Unit};
use crate::game::map_markers::{MapMarkers, TextureValue, marker_name_to_type};
use crate::game::map_objectives::{NoLocale, ObjectiveLocale};
use crate::game::rules::Rules;
use crate::io::json::ColorHex;
use crate::io::json::objectives::ObjectiveMarker;
use crate::logic::enums::{
    CutsceneAction, FetchType, LMarkerControl, MessageType, QueryShape, QueryType, TileLayer,
};
use crate::logic::executor::Executor;
use crate::logic::statement::LogicRule;
use crate::logic::value::{LogicObject, VarRef};

/// `LExecutor.SyncI.syncInterval` quantized to fixed ticks (deviation 4).
pub const SYNC_INTERVAL_TICKS: u64 = 3;
/// `MakeMarkerI.maxMarkers`.
pub const MAKE_MARKER_MAX: usize = 20_000;
/// `Vars.tileSize` as `f32` (world pixels per tile).
const TILE_PX: f32 = crate::config::TILESIZE as f32;
/// `maxTextBuffer` (kept local to avoid an executor import cycle).
pub const MAX_TEXT_BUFFER: usize = 400;

/// `FlushMessageI` blocking state (deviation 5).
///
/// Plan 14 writes these flags when an announcement/toast is presented or
/// expires; headless defaults to no message present (upstream `headless`).
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct MessageState {
    /// An announcement is currently on screen (`ui.hasAnnouncement`).
    pub has_announcement: bool,
    /// A toast is currently on screen (`ui.hudfrag.hasToast`).
    pub has_toast: bool,
    /// `state.rules.mission` visible text.
    pub mission: Option<String>,
}

/// `Call.syncVariable` payload (plan 21 handshake).
#[derive(Clone, Debug, PartialEq)]
pub struct LogicSyncEvent {
    /// Packed building position (`Point2.pack`).
    pub building_pos: i32,
    /// Variable name.
    pub var_name: String,
    /// Variable id in the owning executor (`-1` when unknown).
    pub var_id: i32,
    /// Object-valued flag.
    pub is_obj: bool,
    /// Numeric value when `!is_obj`.
    pub num: f64,
    /// Object value when `is_obj`.
    pub obj: Option<LogicObject>,
}

/// `Call.clientLogicData[Un] reliable` payload (plans 18/21).
#[derive(Clone, Debug, PartialEq)]
pub struct ClientLogicDataEvent {
    /// Channel string.
    pub channel: String,
    /// Object-valued flag.
    pub is_obj: bool,
    /// Numeric value when `!is_obj`.
    pub num: f64,
    /// Object value when `is_obj`.
    pub obj: Option<LogicObject>,
    /// Reliable relay flag.
    pub reliable: bool,
}

/// A deterministic, checksummable privileged-world side effect.
///
/// Entity/audio/FX sinks are recorded rather than applied directly because the
/// VM runs with only a `&mut World` (no `ContentRegistry`/plan-10-17 harness);
/// the owning host consumes [`LogicWorldState::events`] and drives the real sink.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum LogicWorldEvent {
    /// `SetBlockI` on a tile layer.
    SetBlock {
        /// Layer.
        layer: TileLayer,
        /// Target block content/`null` (`@air`).
        block: Option<LogicObject>,
        /// Tile x.
        x: i32,
        /// Tile y.
        y: i32,
        /// Team.
        team: u8,
        /// Clamped rotation.
        rotation: i32,
    },
    /// `SpawnUnitI`.
    SpawnUnit {
        /// Unit type content.
        type_: Option<LogicObject>,
        /// World x (pixels).
        x: f32,
        /// World y (pixels).
        y: f32,
        /// Facing degrees.
        rotation: f32,
        /// Team.
        team: u8,
        /// Spawn-effect toggle.
        effect: bool,
    },
    /// `SpawnBulletI`.
    SpawnBullet {
        /// Bullet source (unit type / turret / content).
        from: Option<LogicObject>,
        /// Weapon/ammo index or item.
        index: Option<LogicObject>,
        /// World x.
        x: f32,
        /// World y.
        y: f32,
        /// Facing degrees.
        rotation: f32,
        /// Team.
        team: u8,
        /// Owner object (`null` = derive from team).
        owner: Option<LogicObject>,
        /// Damage override (`-1` = default).
        damage: f32,
        /// Velocity scale.
        velocity_scl: f32,
        /// Lifetime scale.
        life_scl: f32,
        /// Aim x (world pixels).
        aim_x: f32,
        /// Aim y (world pixels).
        aim_y: f32,
    },
    /// `ApplyEffectI`.
    ApplyStatus {
        /// Clear instead of apply.
        clear: bool,
        /// Status effect content.
        effect: Option<LogicObject>,
        /// Target unit entity index.
        unit_index: u32,
        /// Duration in seconds.
        duration: f32,
    },
    /// `SetWeatherI` (create/fade).
    WeatherSet {
        /// Weather content.
        weather: Option<LogicObject>,
        /// Desired active state.
        state: bool,
    },
    /// `SpawnWaveI`.
    SpawnWave {
        /// Natural wave skip.
        natural: bool,
        /// Spawn x tile.
        x: i32,
        /// Spawn y tile.
        y: i32,
    },
    /// `EffectI`.
    Effect {
        /// `LogicFx` entry name.
        type_name: String,
        /// World x (pixels).
        x: f32,
        /// World y (pixels).
        y: f32,
        /// Rotation (clamped unless the entry rotates).
        rotation: f32,
        /// Raw `Color.toDoubleBits` value.
        color: f64,
        /// Optional data object.
        data: Option<LogicObject>,
    },
    /// `ExplosionI`.
    Explosion {
        /// Team.
        team: u8,
        /// World x (pixels).
        x: f32,
        /// World y (pixels).
        y: f32,
        /// Radius (world pixels, capped at 100 tiles upstream).
        radius: f32,
        /// Damage.
        damage: f32,
        /// Air toggle.
        air: bool,
        /// Ground toggle.
        ground: bool,
        /// Pierce toggle.
        pierce: bool,
        /// Effect toggle.
        effect: bool,
    },
    /// `FlushMessageI` (non-mission, non-headless).
    Message {
        /// Message kind.
        type_: MessageType,
        /// Resolved text.
        text: String,
        /// Duration seconds.
        duration: f32,
    },
    /// `CutsceneI`.
    Cutscene {
        /// Action.
        action: CutsceneAction,
        /// Parameter 1.
        p1: f64,
        /// Parameter 2.
        p2: f64,
        /// Parameter 3.
        p3: f64,
        /// Parameter 4.
        p4: f64,
    },
    /// `SetFlagI` (state change only).
    Flag {
        /// Flag key.
        flag: String,
        /// New value.
        value: bool,
    },
    /// `PlaySoundI`.
    PlaySound {
        /// Positional flag.
        positional: bool,
        /// Resolved numeric sound id.
        id: i32,
        /// Volume (clamped to 2 upstream).
        volume: f32,
        /// Pitch.
        pitch: f32,
        /// Pan.
        pan: f32,
        /// World x.
        x: f32,
        /// World y.
        y: f32,
        /// Limit flag.
        limit: bool,
    },
    /// `PlayMusicI`.
    PlayMusic {
        /// Resolved music name (`null`/empty stops).
        name: String,
        /// Interrupt flag.
        interrupt: bool,
    },
    /// `LocalePrintI`.
    LocalePrint {
        /// Resolved map-locale value.
        value: String,
    },
    /// `SyncI` (`LogicSyncEvent`).
    Sync(Box<LogicSyncEvent>),
    /// `ClientDataI` (`ClientLogicDataEvent`).
    ClientData(Box<ClientLogicDataEvent>),
    /// `FetchI` result summary.
    Fetch {
        /// Fetch kind.
        type_: FetchType,
        /// Team.
        team: u8,
        /// Index.
        index: i32,
        /// Object-valued flag.
        is_obj: bool,
        /// Numeric value when `!is_obj`.
        num: f64,
        /// Object value when `is_obj`.
        obj: Option<LogicObject>,
    },
    /// `QueryI` result summary (`@queries` arena length).
    Query {
        /// Shape.
        shape: QueryShape,
        /// Query type.
        type_: QueryType,
        /// Team filter (0..255) or all teams.
        team: Option<u8>,
        /// World x.
        x: f32,
        /// World y.
        y: f32,
        /// Width / radius.
        w: f32,
        /// Height.
        h: f32,
        /// Result count.
        count: usize,
    },
}

impl LogicWorldEvent {
    /// Stable kind tag (scenario counting / checksum keys).
    pub const fn kind(&self) -> &'static str {
        match self {
            LogicWorldEvent::SetBlock { .. } => "setblock",
            LogicWorldEvent::SpawnUnit { .. } => "spawn",
            LogicWorldEvent::SpawnBullet { .. } => "bullet",
            LogicWorldEvent::ApplyStatus { .. } => "status",
            LogicWorldEvent::WeatherSet { .. } => "weatherset",
            LogicWorldEvent::SpawnWave { .. } => "spawnwave",
            LogicWorldEvent::Effect { .. } => "effect",
            LogicWorldEvent::Explosion { .. } => "explosion",
            LogicWorldEvent::Message { .. } => "message",
            LogicWorldEvent::Cutscene { .. } => "cutscene",
            LogicWorldEvent::Flag { .. } => "flag",
            LogicWorldEvent::PlaySound { .. } => "playsound",
            LogicWorldEvent::PlayMusic { .. } => "playmusic",
            LogicWorldEvent::LocalePrint { .. } => "localeprint",
            LogicWorldEvent::Sync(_) => "sync",
            LogicWorldEvent::ClientData(_) => "clientdata",
            LogicWorldEvent::Fetch { .. } => "fetch",
            LogicWorldEvent::Query { .. } => "query",
        }
    }
}

/// Live rules + markers + event log for the privileged world instructions.
#[derive(Resource, Debug, Clone)]
pub struct LogicWorldState {
    /// Active match rules (plan 12).
    pub rules: Rules,
    /// Map markers (plan 12).
    pub markers: MapMarkers,
    /// Host authority (`offline/host = true`; remote client = false).
    pub is_host: bool,
    /// Fixed 60 Hz tick counter (`Time.millis / (1000/60)`).
    pub tick: u64,
    /// `state.wave`.
    pub wave: i32,
    /// `state.wavetime` (seconds).
    pub wavetime: f32,
    /// Headless preset (no UI): `true` skips announcements and cutscenes.
    pub headless: bool,
    /// Map width in tiles (`world.width()`; 0 = unknown).
    pub map_width: i32,
    /// Map height in tiles (`world.height()`; 0 = unknown).
    pub map_height: i32,
    /// Active weather content ids (`weatherset` state, `weathersense` reads).
    pub weather_active: std::collections::BTreeSet<u16>,
    /// Deterministic sink log (append-only, ordered).
    pub events: Vec<LogicWorldEvent>,
}

impl Default for LogicWorldState {
    fn default() -> Self {
        Self::new()
    }
}

impl LogicWorldState {
    /// Upstream defaults: host authority, headless, no events.
    pub fn new() -> Self {
        Self {
            rules: Rules::default(),
            markers: MapMarkers::new(),
            is_host: true,
            tick: 0,
            wave: 1,
            wavetime: 0.0,
            headless: true,
            map_width: 0,
            map_height: 0,
            weather_active: std::collections::BTreeSet::new(),
            events: Vec::new(),
        }
    }

    /// Appends a side-effect event.
    pub fn emit(&mut self, event: LogicWorldEvent) {
        self.events.push(event);
    }

    /// Number of emitted events of `kind`.
    pub fn count(&self, kind: &str) -> usize {
        self.events.iter().filter(|e| e.kind() == kind).count()
    }

    /// Sim time in milliseconds (tick-derived, never wall clock).
    pub fn time_millis(&self) -> f64 {
        self.tick as f64 * (1000.0 / 60.0)
    }
}

/// Borrows the installed world state, if any.
pub fn state_ref(world: &World) -> Option<&LogicWorldState> {
    world.get_resource::<LogicWorldState>()
}

/// Borrows the installed message state, if any.
pub fn message_ref(world: &World) -> Option<&MessageState> {
    world.get_resource::<MessageState>()
}

/// Content lookups the privileged `ulocate` scans need (`Tile.drop`,
/// `BlockDef.flags`, spawn overlays). Plan 11's `BlockIndexer`/plan 06's
/// `WorldGrid` are not reachable from the VM, so the harness installs this
/// lightweight projection of the content registry at world boot.
#[derive(Resource, Debug, Clone, Default)]
pub struct LogicContentIndex {
    /// `(ore floor/overlay BlockId, dropped ItemId)` (`Tile.drop`).
    pub ore_drops: Vec<(BlockId, ItemId)>,
    /// `(building BlockId, BlockDef.flags)` for `ulocate building`.
    pub block_flags: Vec<(BlockId, Vec<crate::content::registries::blocks::BlockFlag>)>,
    /// Spawn overlay ids (`ulocate spawn`; `BlockKind::SpawnBlock`).
    pub spawn_overlays: Vec<BlockId>,
}

impl LogicContentIndex {
    /// Projects the content registry into the ulocate lookup tables.
    pub fn from_content(content: &crate::content::ContentRegistry) -> Self {
        use crate::content::registries::blocks::BlockKind;
        let mut index = Self::default();
        for def in content.blocks() {
            if let Some(item) = def.item_drop {
                index.ore_drops.push((def.id, item));
            }
            index.block_flags.push((def.id, def.flags.clone()));
            if def.kind == BlockKind::SpawnBlock {
                index.spawn_overlays.push(def.id);
            }
        }
        index
    }

    /// `Tile.drop` for a floor/overlay block, if it is an ore.
    pub fn ore_drop_for(&self, block: BlockId) -> Option<ItemId> {
        self.ore_drops
            .iter()
            .find(|(id, _)| *id == block)
            .map(|(_, item)| *item)
    }

    /// `BlockDef.flags` for a building block.
    pub fn flags_of(&self, block: BlockId) -> &[crate::content::registries::blocks::BlockFlag] {
        self.block_flags
            .iter()
            .find(|(id, _)| *id == block)
            .map(|(_, flags)| flags.as_slice())
            .unwrap_or(&[])
    }

    /// Whether `block` is a spawn overlay (`BlockKind::SpawnBlock`).
    pub fn is_spawn_overlay(&self, block: BlockId) -> bool {
        self.spawn_overlays.contains(&block)
    }
}

/// Borrows the installed content index, if any.
pub fn content_index_ref(world: &World) -> Option<&LogicContentIndex> {
    world.get_resource::<LogicContentIndex>()
}

/// Builds a default objective marker for a `MapObjectives.markerNameToType` name.
pub fn new_marker(type_name: &str) -> Option<ObjectiveMarker> {
    let canonical = marker_name_to_type(type_name)?;
    Some(match canonical {
        "point" => ObjectiveMarker::Point(Default::default()),
        "shapeText" => ObjectiveMarker::ShapeText(Default::default()),
        "shape" => ObjectiveMarker::Shape(Default::default()),
        "text" => ObjectiveMarker::Text(Default::default()),
        "line" => ObjectiveMarker::Line(Default::default()),
        "texture" => ObjectiveMarker::Texture(Default::default()),
        "quad" => ObjectiveMarker::Quad(Default::default()),
        "light" => ObjectiveMarker::Light(Default::default()),
        _ => return None,
    })
}

/// Stable string key for a logic object (checksums/event assertions).
pub fn object_key(obj: &LogicObject) -> String {
    match obj {
        LogicObject::Building(e) => format!("building:{}", e.index().index()),
        LogicObject::Unit(e) => format!("unit:{}", e.index().index()),
        LogicObject::Content(c) => format!("content:{}:{}", c.type_.name(), c.id),
        LogicObject::Team(team) => format!("team:{team}"),
        LogicObject::Str(s) => format!("str:{s}"),
        LogicObject::Enum(name) => format!("enum:{name}"),
        LogicObject::Align(value) => format!("align:{value}"),
        LogicObject::Query(id) => format!("query:{id}"),
    }
}

/// Optional object key.
pub fn option_key(obj: Option<&LogicObject>) -> String {
    match obj {
        Some(obj) => object_key(obj),
        None => "null".to_owned(),
    }
}

/// `Color.fromDouble`: the low 32 raw bits as rgba (plan 12 helper parity).
pub fn color_from_double(value: f64) -> ColorHex {
    ColorHex(crate::content::Rgba::from_rgba8888(value.to_bits() as u32))
}

/// `ObjectiveMarker.control` fetch-text locale (pass-through).
pub fn no_locale() -> NoLocale {
    NoLocale
}

/// Reads a variable's current value into `(is_obj, num, obj)`.
pub fn read_var(
    exec: &crate::logic::executor::Executor,
    var: VarRef,
) -> (bool, f64, Option<LogicObject>) {
    let cell = exec.arena.get(var.id());
    (cell.is_obj, cell.num, cell.obj.clone())
}

/// Stable `EntitySeq` ordering key (falls back to the entity index).
fn seq_of(world: &World, e: Entity) -> u64 {
    world
        .get::<crate::ecs::EntitySeq>(e)
        .map(|seq| seq.0)
        .unwrap_or(u64::MAX)
}

/// Object ordering key for deterministic tie-breaks.
fn obj_index(obj: &LogicObject) -> u32 {
    match obj {
        LogicObject::Unit(e) | LogicObject::Building(e) => e.index().index(),
        _ => u32::MAX,
    }
}

/// Axis-aligned world-space box test.
fn in_box(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32) -> bool {
    px >= x && px <= x + w && py >= y && py <= y + h
}

/// Circle test (`within`).
fn within(cx: f32, cy: f32, radius: f32, px: f32, py: f32) -> bool {
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= radius * radius
}

fn num(exec: &Executor, var: VarRef) -> f64 {
    exec.arena.get(var.id()).num()
}

fn numi(exec: &Executor, var: VarRef) -> i32 {
    exec.arena.get(var.id()).numi()
}

fn bool_of(exec: &Executor, var: VarRef) -> bool {
    exec.arena.get(var.id()).as_bool()
}

fn obj_of(exec: &Executor, var: VarRef) -> Option<LogicObject> {
    exec.arena.get(var.id()).value_obj().cloned()
}

fn out_obj(exec: &mut Executor, var: VarRef, value: Option<LogicObject>) {
    exec.set_obj(var.id(), value);
}

/// `QueryI` — filters units/buildings into the `@queries` arena (deviation 3).
#[allow(clippy::too_many_arguments)]
pub fn run_query(
    exec: &mut Executor,
    world: &mut World,
    shape: QueryShape,
    type_: QueryType,
    team: VarRef,
    x: VarRef,
    y: VarRef,
    w: VarRef,
    h: VarRef,
) {
    if exec.query_result.is_none() {
        return;
    }
    let team_filter = exec.arena.get(team.id()).team();
    let wx = exec.arena.get(x.id()).numf_world();
    let wy = exec.arena.get(y.id()).numf_world();
    let ww = exec.arena.get(w.id()).numf_world();
    let wh = exec.arena.get(h.id()).numf_world();
    let radius = ww;
    let (rx, ry, rw, rh) = if shape == QueryShape::Circle {
        (wx - radius, wy - radius, radius * 2.0, radius * 2.0)
    } else {
        (wx, wy, ww, wh)
    };

    let mut found: Vec<LogicObject> = Vec::new();
    if type_ == QueryType::Unit {
        let mut query =
            world.query_filtered::<(Entity, &Pos, &TeamComp, Option<&HitboxComp>), With<Unit>>();
        for (e, pos, t, hit) in query.iter(world) {
            if team_filter.is_some_and(|want| t.team != want) {
                continue;
            }
            if !in_box(pos.x, pos.y, rx, ry, rw, rh) {
                continue;
            }
            if shape == QueryShape::Circle {
                let hr = hit.map(|h| h.hit_size / 2.0).unwrap_or(0.0);
                if !within(wx, wy, radius + hr, pos.x, pos.y) {
                    continue;
                }
            }
            found.push(LogicObject::Unit(e));
        }
    } else if type_ == QueryType::Building {
        let candidates: Vec<(Entity, i32, i32, u8)> = {
            let mut query =
                world.query_filtered::<(Entity, &Building, &TeamComp), With<Building>>();
            query
                .iter(world)
                .map(|(e, b, t)| (e, b.tile.x() as i32, b.tile.y() as i32, t.team))
                .collect()
        };
        for (e, tx, ty, team) in candidates {
            if team_filter.is_some_and(|want| team != want) {
                continue;
            }
            let size = block_size(world, e);
            let cx = (tx as f32 + (size - 1) as f32 / 2.0 + 0.5) * TILE_PX;
            let cy = (ty as f32 + (size - 1) as f32 / 2.0 + 0.5) * TILE_PX;
            if !in_box(cx, cy, rx, ry, rw, rh) {
                continue;
            }
            if shape == QueryShape::Circle
                && !within(wx, wy, radius + TILE_PX * size as f32 / 2.0, cx, cy)
            {
                continue;
            }
            found.push(LogicObject::Building(e));
        }
    }
    found.sort_by_key(obj_index);
    let count = found.len();
    exec.queries[0] = found;

    if let Some(mut state) = world.get_resource_mut::<LogicWorldState>() {
        state.emit(LogicWorldEvent::Query {
            shape,
            type_,
            team: team_filter,
            x: wx,
            y: wy,
            w: ww,
            h: wh,
            count,
        });
    }
}

/// `FetchI` — team lists/counts over the ECS (plan-11 `TeamData` subset).
pub fn run_fetch(
    exec: &mut Executor,
    world: &mut World,
    type_: FetchType,
    result: VarRef,
    team: VarRef,
    index: VarRef,
    extra: VarRef,
) {
    let Some(team_id) = exec.arena.get(team.id()).team() else {
        return;
    };
    let i = exec.arena.get(index.id()).numi();
    let extra_obj = exec.arena.get(extra.id()).value_obj().cloned();

    let (is_obj, numv, objv) = match type_ {
        FetchType::Unit | FetchType::UnitCount => {
            let type_filter = match &extra_obj {
                Some(LogicObject::Content(c)) if c.type_ == ContentType::Unit => {
                    Some(UnitTypeId::new(c.id))
                }
                _ => None,
            };
            let mut units: Vec<Entity> = {
                let mut query = world
                    .query_filtered::<(Entity, &TeamComp, Option<&UnitTypeComp>), With<Unit>>();
                query
                    .iter(world)
                    .filter(|(_, t, ut)| {
                        t.team == team_id
                            && type_filter.is_none_or(|want| ut.is_some_and(|u| u.type_id == want))
                    })
                    .map(|(e, _, _)| e)
                    .collect()
            };
            units.sort_by_key(|e| (seq_of(world, *e), e.index()));
            if type_ == FetchType::Unit {
                let value = if i < 0 {
                    None
                } else {
                    units.get(i as usize).copied()
                };
                (true, 0.0, value.map(LogicObject::Unit))
            } else {
                (false, units.len() as f64, None)
            }
        }
        FetchType::Build | FetchType::BuildCount => {
            let block_filter = match &extra_obj {
                Some(LogicObject::Content(c)) if c.type_ == ContentType::Block => {
                    Some(BlockId::new(c.id))
                }
                _ => None,
            };
            let mut builds: Vec<Entity> = {
                let mut query =
                    world.query_filtered::<(Entity, &Building, &TeamComp), With<Building>>();
                query
                    .iter(world)
                    .filter(|(_, b, t)| {
                        t.team == team_id && block_filter.is_none_or(|want| b.block == want)
                    })
                    .map(|(e, _, _)| e)
                    .collect()
            };
            builds.sort_by_key(|e| (seq_of(world, *e), e.index()));
            if type_ == FetchType::Build {
                let value = if i < 0 {
                    None
                } else {
                    builds.get(i as usize).copied()
                };
                (true, 0.0, value.map(LogicObject::Building))
            } else {
                (false, builds.len() as f64, None)
            }
        }
        // Core/player lists need plan 11/12 `TeamData` caches (deferred).
        FetchType::Core | FetchType::Player => (true, 0.0, None),
        FetchType::CoreCount | FetchType::PlayerCount => (false, 0.0, None),
    };

    if is_obj {
        out_obj(exec, result, objv.clone());
    } else {
        exec.set_num(result.id(), numv);
    }
    if let Some(mut state) = world.get_resource_mut::<LogicWorldState>() {
        state.emit(LogicWorldEvent::Fetch {
            type_,
            team: team_id,
            index: i,
            is_obj,
            num: numv,
            obj: objv,
        });
    }
}

/// Block size in tiles of a building (1 when unknown).
fn block_size(world: &World, e: Entity) -> i32 {
    let Some(block) = world.get::<Building>(e).map(|b| b.block) else {
        return 1;
    };
    world
        .get_resource::<crate::world::BlockTable>()
        .and_then(|t| t.get(block).map(|inst| inst.def.size))
        .unwrap_or(1)
}

/// `GetBlockI` — block/building layers from the `TileBuilds` mirror.
///
/// Floor/ore need the `WorldGrid`, which is not a world resource (deferred to
/// plan 06/07); those layers return `null`.
pub fn run_get_block(
    exec: &mut Executor,
    world: &mut World,
    layer: TileLayer,
    result: VarRef,
    x: VarRef,
    y: VarRef,
) {
    let tx = exec.arena.get(x.id()).numi();
    let ty = exec.arena.get(y.id()).numi();
    // The generation/editor world carries the live `WorldGrid`; read floor/ore
    // and block straight from it (plan 06 filter hook).
    if let Some(grid) = world.get_resource::<crate::world::WorldGrid>() {
        let in_bounds = tx >= 0 && ty >= 0 && tx < grid.width() && ty < grid.height();
        let value = if in_bounds {
            let tile = grid.tile(tx, ty);
            match layer {
                TileLayer::Building => tile.build.map(LogicObject::Building),
                TileLayer::Block => Some(LogicObject::Content(ContentRef::block(tile.block))),
                TileLayer::Floor => Some(LogicObject::Content(ContentRef::block(tile.floor))),
                TileLayer::Ore => Some(LogicObject::Content(ContentRef::block(tile.overlay))),
            }
        } else {
            None
        };
        out_obj(exec, result, value);
        return;
    }
    let builds = world.get_resource::<crate::world::TileBuilds>();
    let in_bounds = builds.is_some_and(|b| tx >= 0 && ty >= 0 && tx < b.width && ty < b.height);
    let entity = builds.and_then(|b| b.get(tx, ty));
    let value = match layer {
        TileLayer::Building => entity.map(LogicObject::Building),
        TileLayer::Block => match entity {
            Some(e) => world
                .get::<Building>(e)
                .map(|b| LogicObject::Content(ContentRef::block(b.block))),
            None if in_bounds => Some(LogicObject::Content(ContentRef::block(BlockId::AIR))),
            None => None,
        },
        TileLayer::Floor | TileLayer::Ore => None,
    };
    out_obj(exec, result, value);
}

/// `SetBlockI` — host-gated; records the mutation for the owning host.
#[allow(clippy::too_many_arguments)]
pub fn run_set_block(
    exec: &mut Executor,
    world: &mut World,
    layer: TileLayer,
    block: VarRef,
    x: VarRef,
    y: VarRef,
    team: VarRef,
    rotation: VarRef,
) {
    let tx = numi(exec, x);
    let ty = numi(exec, y);
    if tx < 0 || ty < 0 {
        return;
    }
    let block_obj = obj_of(exec, block);
    let team_id = exec.arena.get(team.id()).team().unwrap_or_else(|| {
        world
            .get_resource::<LogicWorldState>()
            .map(|s| s.rules.default_team)
            .unwrap_or(0)
    });
    let rot = numi(exec, rotation).clamp(0, 3);

    // Map-generation/editor worlds carry the live `WorldGrid`; apply the
    // mutation in place there (`LogicFilter` setting tiles, plan 06 §3.8).
    if let Some(mut grid) = world.get_resource_mut::<crate::world::WorldGrid>() {
        let width = grid.width();
        let height = grid.height();
        if tx < width && ty < height {
            let block_id = match &block_obj {
                Some(LogicObject::Content(c)) if c.type_ == ContentType::Block => {
                    Some(BlockId::new(c.id))
                }
                _ => None,
            };
            if let Some(block_id) = block_id {
                let index = (tx + ty * width) as usize;
                let tile = grid.tiles.geti_mut(index);
                match layer {
                    TileLayer::Ore => tile.overlay = block_id,
                    TileLayer::Floor => tile.floor = block_id,
                    TileLayer::Block => {
                        tile.block = block_id;
                        tile.build = None;
                    }
                    TileLayer::Building => {}
                }
            }
        }
    }

    // Host-gated event log (the normal processor path).
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    if !state.is_host {
        return;
    }
    state.emit(LogicWorldEvent::SetBlock {
        layer,
        block: block_obj,
        x: tx,
        y: ty,
        team: team_id,
        rotation: rot,
    });
}

/// `SpawnUnitI` — host-gated; records the spawn (plan 11 sink applied by host).
#[allow(clippy::too_many_arguments)]
pub fn run_spawn_unit(
    exec: &mut Executor,
    world: &mut World,
    type_: VarRef,
    x: VarRef,
    y: VarRef,
    rotation: VarRef,
    team: VarRef,
    _result: VarRef,
    effect: VarRef,
) {
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    if !state.is_host {
        return;
    }
    let Some(team_id) = exec.arena.get(team.id()).team() else {
        return;
    };
    let unit_type = obj_of(exec, type_);
    state.emit(LogicWorldEvent::SpawnUnit {
        type_: unit_type,
        x: exec.arena.get(x.id()).numf_world(),
        y: exec.arena.get(y.id()).numf_world(),
        rotation: num(exec, rotation) as f32,
        team: team_id,
        effect: bool_of(exec, effect),
    });
}

/// `SpawnBulletI` — records the resolved bullet request (plan 10 sink).
#[allow(clippy::too_many_arguments)]
pub fn run_spawn_bullet(
    exec: &mut Executor,
    world: &mut World,
    _result: VarRef,
    from: VarRef,
    index: VarRef,
    x: VarRef,
    y: VarRef,
    rotation: VarRef,
    team: VarRef,
    owner: VarRef,
    damage: VarRef,
    velocity_scl: VarRef,
    life_scl: VarRef,
    aim_x: VarRef,
    aim_y: VarRef,
) {
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    if !state.is_host {
        return;
    }
    let owner_obj = obj_of(exec, owner);
    let team_id = match exec.arena.get(team.id()).team() {
        Some(team) => team,
        None => crate::game::team::DERELICT.0,
    };
    state.emit(LogicWorldEvent::SpawnBullet {
        from: obj_of(exec, from),
        index: obj_of(exec, index),
        x: exec.arena.get(x.id()).numf_world(),
        y: exec.arena.get(y.id()).numf_world(),
        rotation: num(exec, rotation) as f32,
        team: team_id,
        owner: owner_obj,
        damage: num(exec, damage) as f32,
        velocity_scl: num(exec, velocity_scl) as f32,
        life_scl: num(exec, life_scl) as f32,
        aim_x: exec.arena.get(aim_x.id()).numf_world(),
        aim_y: exec.arena.get(aim_y.id()).numf_world(),
    });
}

/// `ApplyEffectI` — host-gated status apply/clear request (plan 11 sink).
pub fn run_apply_status(
    exec: &mut Executor,
    world: &mut World,
    clear: bool,
    effect: VarRef,
    unit: VarRef,
    duration: VarRef,
) {
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    if !state.is_host {
        return;
    }
    let Some(LogicObject::Unit(entity)) = obj_of(exec, unit) else {
        return;
    };
    state.emit(LogicWorldEvent::ApplyStatus {
        clear,
        effect: obj_of(exec, effect),
        unit_index: entity.index().index(),
        duration: num(exec, duration) as f32,
    });
}

/// `SenseWeatherI`.
pub fn run_weather_sense(exec: &mut Executor, world: &mut World, to: VarRef, weather: VarRef) {
    let active = obj_of(exec, weather)
        .and_then(|obj| match obj {
            LogicObject::Content(c) if c.type_ == ContentType::Weather => Some(c.id),
            _ => None,
        })
        .is_some_and(|id| {
            world
                .get_resource::<LogicWorldState>()
                .is_some_and(|state| state.weather_active.contains(&id))
        });
    exec.set_num(to.id(), if active { 1.0 } else { 0.0 });
}

/// `SetWeatherI`.
pub fn run_weather_set(exec: &mut Executor, world: &mut World, weather: VarRef, state_var: VarRef) {
    let Some(id) = obj_of(exec, weather).and_then(|obj| match obj {
        LogicObject::Content(c) if c.type_ == ContentType::Weather => Some(c.id),
        _ => None,
    }) else {
        return;
    };
    let on = bool_of(exec, state_var);
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    if on {
        state.weather_active.insert(id);
    } else {
        state.weather_active.remove(&id);
    }
    state.emit(LogicWorldEvent::WeatherSet {
        weather: Some(LogicObject::Content(ContentRef::new(
            ContentType::Weather,
            id,
        ))),
        state: on,
    });
}

/// `SpawnWaveI` — host-gated.
pub fn run_spawn_wave(
    exec: &mut Executor,
    world: &mut World,
    x: VarRef,
    y: VarRef,
    natural: VarRef,
) {
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    if !state.is_host {
        return;
    }
    state.emit(LogicWorldEvent::SpawnWave {
        natural: bool_of(exec, natural),
        x: numi(exec, x),
        y: numi(exec, y),
    });
}

/// `SetRuleI` (plan 12 `Rules` + local wave/wavetime/map-area state).
#[allow(clippy::too_many_arguments)]
pub fn run_set_rule(
    exec: &mut Executor,
    world: &mut World,
    rule: LogicRule,
    value: VarRef,
    p1: VarRef,
    p2: VarRef,
    p3: VarRef,
    p4: VarRef,
) {
    let value = exec.arena.get(value.id()).clone();
    let p1 = exec.arena.get(p1.id()).clone();
    let p2 = exec.arena.get(p2.id()).clone();
    let p3 = exec.arena.get(p3.id()).clone();
    let p4 = exec.arena.get(p4.id()).clone();
    if let Some(mut state) = world.get_resource_mut::<LogicWorldState>() {
        crate::logic::rules::apply_to_state(&mut state, rule, &value, &p1, &p2, &p3, &p4);
    }
}

/// `FlushMessageI` (deviation 5 `MessageState` blocking).
pub fn run_flush_message(
    exec: &mut Executor,
    world: &mut World,
    type_: MessageType,
    duration: VarRef,
    out_success: VarRef,
) {
    // Default to success.
    exec.set_num(out_success.id(), 1.0);
    let headless = world
        .get_resource::<LogicWorldState>()
        .map(|state| state.headless)
        .unwrap_or(true);
    if headless && type_ != MessageType::Mission {
        exec.text_buffer.clear();
        return;
    }

    let (has_announcement, has_toast) = world
        .get_resource::<MessageState>()
        .map(|m| (m.has_announcement, m.has_toast))
        .unwrap_or((false, false));
    let blocking = (type_ == MessageType::Announce && has_announcement)
        || (type_ == MessageType::Notify && has_toast)
        || (type_ == MessageType::Toast && has_announcement);
    if blocking {
        let waiting = exec.arena.get(out_success.id()).name == "@wait";
        if waiting {
            let counter = exec.counter;
            let cur = exec.arena.get(counter).num;
            exec.set_num(counter, cur - 1.0);
            exec.yielded = true;
        } else {
            exec.set_num(out_success.id(), 0.0);
        }
        return;
    }

    let mut text = exec.text_buffer.clone();
    if let Some(stripped) = text.strip_prefix('@') {
        text = stripped.to_owned();
    }
    let duration_s = num(exec, duration) as f32;
    exec.text_buffer.clear();

    if let Some(mut state) = world.get_resource_mut::<LogicWorldState>() {
        if type_ == MessageType::Mission {
            state.rules.mission = Some(text.clone());
        } else {
            state.emit(LogicWorldEvent::Message {
                type_,
                text: text.clone(),
                duration: duration_s,
            });
        }
    }
}

/// `CutsceneI` — emits a camera/HUD request (headless rendering is plan 16).
pub fn run_cutscene(
    exec: &mut Executor,
    world: &mut World,
    action: CutsceneAction,
    p1: VarRef,
    p2: VarRef,
    p3: VarRef,
    p4: VarRef,
) {
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    state.emit(LogicWorldEvent::Cutscene {
        action,
        p1: num(exec, p1),
        p2: num(exec, p2),
        p3: num(exec, p3),
        p4: num(exec, p4),
    });
}

/// `EffectI`.
#[allow(clippy::too_many_arguments)]
pub fn run_effect(
    exec: &mut Executor,
    world: &mut World,
    effect: Option<crate::logic::fx::EffectEntry>,
    x: VarRef,
    y: VarRef,
    rotation: VarRef,
    color: VarRef,
    data: VarRef,
) {
    let Some(entry) = effect else {
        return;
    };
    let rotation_f = num(exec, rotation) as f32;
    let rot = if entry.rotate {
        rotation_f
    } else {
        rotation_f.min(1000.0)
    };
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    state.emit(LogicWorldEvent::Effect {
        type_name: entry.name.to_owned(),
        x: exec.arena.get(x.id()).numf_world(),
        y: exec.arena.get(y.id()).numf_world(),
        rotation: rot,
        color: num(exec, color),
        data: obj_of(exec, data),
    });
}

/// `ExplosionI` — host-gated (damage/FX applied by plan 10/17 host).
#[allow(clippy::too_many_arguments)]
pub fn run_explosion(
    exec: &mut Executor,
    world: &mut World,
    team: VarRef,
    x: VarRef,
    y: VarRef,
    radius: VarRef,
    damage: VarRef,
    air: VarRef,
    ground: VarRef,
    pierce: VarRef,
    effect: VarRef,
) {
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    if !state.is_host {
        return;
    }
    let team_id = exec
        .arena
        .get(team.id())
        .team()
        .unwrap_or(crate::game::team::DERELICT.0);
    // Radius cap: 100 tiles.
    let radius_tiles = (num(exec, radius) as f32).min(100.0) * TILE_PX;
    let damage_v = num(exec, damage) as f32;
    if damage_v < 0.0 {
        return;
    }
    state.emit(LogicWorldEvent::Explosion {
        team: team_id,
        x: exec.arena.get(x.id()).numf_world(),
        y: exec.arena.get(y.id()).numf_world(),
        radius: radius_tiles,
        damage: damage_v,
        air: bool_of(exec, air),
        ground: bool_of(exec, ground),
        pierce: bool_of(exec, pierce),
        effect: bool_of(exec, effect),
    });
}

/// `GetFlagI`.
pub fn run_get_flag(exec: &mut Executor, world: &mut World, result: VarRef, flag: VarRef) {
    match obj_of(exec, flag) {
        Some(LogicObject::Str(name)) => {
            let present = world
                .get_resource::<LogicWorldState>()
                .is_some_and(|state| state.rules.objective_flags.contains(&name));
            exec.set_num(result.id(), if present { 1.0 } else { 0.0 });
        }
        _ => out_obj(exec, result, None),
    }
}

/// `SetFlagI` — only mutates when the flag state actually changes.
pub fn run_set_flag(exec: &mut Executor, world: &mut World, flag: VarRef, value: VarRef) {
    let Some(LogicObject::Str(name)) = obj_of(exec, flag) else {
        return;
    };
    let on = bool_of(exec, value);
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    let present = state.rules.objective_flags.contains(&name);
    if present == on {
        return;
    }
    if on {
        state.rules.objective_flags.insert(name.clone());
    } else {
        state.rules.objective_flags.shift_remove(&name);
    }
    state.emit(LogicWorldEvent::Flag {
        flag: name,
        value: on,
    });
}

/// `SetMarkerI` against plan 12's `MapMarkers`.
pub fn run_set_marker(
    exec: &mut Executor,
    world: &mut World,
    type_: LMarkerControl,
    id: VarRef,
    p1: VarRef,
    p2: VarRef,
    p3: VarRef,
) {
    let mid = numi(exec, id);
    if mid < 0 {
        return;
    }
    let id = mid as u32;
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    if type_ == LMarkerControl::Remove {
        state.markers.remove(id);
        return;
    }
    if state.markers.get(id).is_none() {
        return;
    }
    if type_ == LMarkerControl::FlushText {
        let text = exec.text_buffer.clone();
        state
            .markers
            .set_text(id, &text, bool_of(exec, p1), &no_locale());
        exec.text_buffer.clear();
    } else if type_ == LMarkerControl::Texture {
        if bool_of(exec, p1) {
            let text = exec.text_buffer.clone();
            state.markers.set_texture(id, TextureValue::String(text));
            exec.text_buffer.clear();
        } else if let Some(LogicObject::Str(texture)) = obj_of(exec, p2) {
            state.markers.set_texture(id, TextureValue::String(texture));
        }
    } else {
        state.markers.control(
            id,
            type_,
            exec.arena.get(p1.id()).num_or_nan(),
            exec.arena.get(p2.id()).num_or_nan(),
            exec.arena.get(p3.id()).num_or_nan(),
        );
    }
}

/// `MakeMarkerI`.
pub fn run_make_marker(
    exec: &mut Executor,
    world: &mut World,
    type_: &str,
    id: VarRef,
    x: VarRef,
    y: VarRef,
    replace: VarRef,
) {
    let Some(factory) = new_marker(type_) else {
        return;
    };
    let mid = numi(exec, id);
    if mid < 0 {
        return;
    }
    let mid = mid as u32;
    let replace = bool_of(exec, replace);
    let xv = exec.arena.get(x.id()).num_or_nan();
    let yv = exec.arena.get(y.id()).num_or_nan();
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    if state.markers.size() >= MAKE_MARKER_MAX {
        return;
    }
    if replace || !state.markers.has(mid) {
        state.markers.add(mid, factory);
        state.markers.control(mid, LMarkerControl::Pos, xv, yv, 0.0);
    }
}

/// `PlaySoundI` (plan 18 `SoundId` resolution is a host sink).
#[allow(clippy::too_many_arguments)]
pub fn run_play_sound(
    exec: &mut Executor,
    world: &mut World,
    positional: bool,
    id: VarRef,
    volume: VarRef,
    pitch: VarRef,
    pan: VarRef,
    x: VarRef,
    y: VarRef,
    limit: VarRef,
) {
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    state.emit(LogicWorldEvent::PlaySound {
        positional,
        id: numi(exec, id),
        volume: (num(exec, volume) as f32).min(2.0),
        pitch: num(exec, pitch) as f32,
        pan: num(exec, pan) as f32,
        x: exec.arena.get(x.id()).numf_world(),
        y: exec.arena.get(y.id()).numf_world(),
        limit: bool_of(exec, limit),
    });
}

/// `PlayMusicI`.
pub fn run_play_music(exec: &mut Executor, world: &mut World, name: VarRef, interrupt: VarRef) {
    let resolved = match obj_of(exec, name) {
        Some(LogicObject::Str(s)) => s,
        _ => String::new(),
    };
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    state.emit(LogicWorldEvent::PlayMusic {
        name: resolved,
        interrupt: bool_of(exec, interrupt),
    });
}

/// `LocalePrintI` (appends to the text buffer; records for tests).
pub fn run_locale_print(exec: &mut Executor, world: &mut World, name: VarRef) {
    if exec.text_buffer.chars().count() >= MAX_TEXT_BUFFER {
        return;
    }
    let Some(LogicObject::Str(key)) = obj_of(exec, name) else {
        return;
    };
    let value = no_locale().fetch_text(&key);
    exec.text_buffer.push_str(&value);
    if let Some(mut state) = world.get_resource_mut::<LogicWorldState>() {
        state.emit(LogicWorldEvent::LocalePrint { value });
    }
}

/// `SyncI` — throttled by sim ticks (`SYNC_INTERVAL_TICKS`, deviation 4).
pub fn run_sync(exec: &mut Executor, world: &mut World, variable: VarRef) {
    let Some(build) = exec.build else {
        return;
    };
    let now = match world.get_resource::<LogicWorldState>() {
        Some(state) => state.time_millis(),
        None => return,
    };
    let cell = exec.arena.get(variable.id());
    if cell.constant {
        return;
    }
    if now - cell.synced_at < (SYNC_INTERVAL_TICKS as f64) * (1000.0 / 60.0) {
        return;
    }
    let is_obj = cell.is_obj;
    let numv = cell.num;
    let objv = cell.obj.clone();
    let var_name = cell.name.clone();
    let var_id = cell.id;
    exec.arena.get_mut(variable.id()).synced_at = now;
    let building_pos = world
        .get::<Building>(build)
        .map(|b| b.tile.pack())
        .unwrap_or(0);
    if let Some(mut state) = world.get_resource_mut::<LogicWorldState>() {
        state.emit(LogicWorldEvent::Sync(Box::new(LogicSyncEvent {
            building_pos,
            var_name,
            var_id,
            is_obj,
            num: numv,
            obj: objv,
        })));
    }
}

/// `ClientDataI` — gated by `rules.allowLogicData` at run time.
pub fn run_client_data(
    exec: &mut Executor,
    world: &mut World,
    channel: VarRef,
    value: VarRef,
    reliable: VarRef,
) {
    let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else {
        return;
    };
    if !state.rules.allow_logic_data {
        return;
    }
    let Some(LogicObject::Str(channel)) = obj_of(exec, channel) else {
        return;
    };
    let is_obj = exec.arena.get(value.id()).is_obj;
    let numv = exec.arena.get(value.id()).num;
    let objv = exec.arena.get(value.id()).obj.clone();
    state.emit(LogicWorldEvent::ClientData(Box::new(
        ClientLogicDataEvent {
            channel,
            is_obj,
            num: numv,
            obj: objv,
            reliable: bool_of(exec, reliable),
        },
    )));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_state_defaults_are_host_headless() {
        let state = LogicWorldState::new();
        assert!(state.is_host);
        assert!(state.headless);
        assert_eq!(state.wave, 1);
        assert!(state.events.is_empty());
    }

    #[test]
    fn event_kinds_and_counts() {
        let mut state = LogicWorldState::new();
        state.emit(LogicWorldEvent::LocalePrint {
            value: "x".to_owned(),
        });
        state.emit(LogicWorldEvent::Flag {
            flag: "f".to_owned(),
            value: true,
        });
        assert_eq!(state.count("localeprint"), 1);
        assert_eq!(state.count("flag"), 1);
        assert_eq!(state.count("sync"), 0);
        assert_eq!(state.time_millis(), 0.0);
    }

    #[test]
    fn marker_factory_matches_registry_names() {
        assert!(matches!(
            new_marker("point"),
            Some(ObjectiveMarker::Point(_))
        ));
        assert!(matches!(
            new_marker("Minimap"),
            Some(ObjectiveMarker::Point(_))
        ));
        assert!(new_marker("nope").is_none());
    }

    #[test]
    fn object_keys_are_stable() {
        assert_eq!(object_key(&LogicObject::Str("hi".into())), "str:hi");
        assert_eq!(object_key(&LogicObject::Team(3)), "team:3");
        assert_eq!(option_key(None), "null");
    }

    use crate::content::{BlockId, UnitTypeId};
    use crate::ecs::EntitySeq;
    use crate::entities::comp::unit::{HitboxComp, UnitTypeComp};
    use crate::entities::comp::{Building, Pos, TeamComp, Unit};
    use crate::logic::assembler::Assembler;
    use crate::world::TilePos;

    fn run_code(world: &mut World, code: &str) -> Executor {
        let asm = Assembler::assemble(code, true).expect("assemble");
        let mut exec = Executor::new();
        exec.privileged = true;
        exec.load(asm);
        exec.run(world, 100);
        exec
    }

    #[test]
    fn setrule_flags_and_message_apply_to_state() {
        let mut world = World::new();
        world.insert_resource(LogicWorldState::new());
        world.insert_resource(MessageState::default());
        let code = "print \"hi\"\n\
            message mission 3 @wait\n\
            setrule waves false\n\
            setrule waveSpacing 4 0 0 100 100\n\
            setflag \"captured\" true\n\
            getflag got \"captured\"\n\
            stop\n";
        let exec = run_code(&mut world, code);
        let state = world.get_resource::<LogicWorldState>().expect("state");
        assert!(!state.rules.waves);
        assert_eq!(state.rules.wave_spacing, 240.0);
        assert!(state.rules.objective_flags.contains("captured"));
        assert_eq!(state.rules.mission.as_deref(), Some("hi"));
        let got = exec.optional_var("got").map(|id| exec.arena.get(id).num());
        assert_eq!(got, Some(1.0));
        assert_eq!(state.count("flag"), 1);
    }

    #[test]
    fn query_and_fetch_see_ecs_units() {
        let mut world = World::new();
        world.insert_resource(LogicWorldState::new());
        let unit = world
            .spawn((
                Unit,
                Pos { x: 40.0, y: 40.0 },
                TeamComp { team: 0 },
                HitboxComp { hit_size: 8.0 },
                UnitTypeComp {
                    type_id: UnitTypeId::new(1),
                },
                EntitySeq(0),
            ))
            .id();
        let code = "query rect unit 0 0 0 20 20\n\
            read u @queries 0\n\
            fetch unitCount fc 0 0 @conveyor\n\
            stop\n";
        let exec = run_code(&mut world, code);
        let u = exec
            .optional_var("u")
            .and_then(|id| exec.arena.get(id).obj.clone());
        assert_eq!(u, Some(LogicObject::Unit(unit)));
        let fc = exec.optional_var("fc").map(|id| exec.arena.get(id).num());
        assert_eq!(fc, Some(1.0));
        let state = world.get_resource::<LogicWorldState>().expect("state");
        assert_eq!(state.count("query"), 1);
        assert_eq!(state.count("fetch"), 1);
    }

    #[test]
    fn setblock_and_explosion_are_host_gated() {
        let mut world = World::new();
        world.insert_resource(LogicWorldState::new());
        let code = "setblock block @copper-wall 7 7 0 0\n\
            explosion 0 40 40 3 20 true true false true\n\
            stop\n";
        run_code(&mut world, code);
        let state = world.get_resource::<LogicWorldState>().expect("state");
        assert_eq!(state.count("setblock"), 1);
        assert_eq!(state.count("explosion"), 1);

        // A remote client (not host) records nothing.
        let mut remote = World::new();
        let mut remote_state = LogicWorldState::new();
        remote_state.is_host = false;
        remote.insert_resource(remote_state);
        run_code(&mut remote, code);
        let state = remote.get_resource::<LogicWorldState>().expect("state");
        assert_eq!(state.count("setblock"), 0);
        assert_eq!(state.count("explosion"), 0);
    }

    #[test]
    fn makemarker_creates_and_controls_a_marker() {
        let mut world = World::new();
        world.insert_resource(LogicWorldState::new());
        let code = "makemarker shape 1 5 6 true\n\
            setmarker pos 1 3 4 0\n\
            setmarker radius 1 9 -1 -1\n\
            setmarker world 1 1 -1 -1\n\
            stop\n";
        run_code(&mut world, code);
        let state = world.get_resource::<LogicWorldState>().expect("state");
        assert_eq!(state.markers.size(), 1);
        assert!(state.markers.world_markers.contains(&1));
        match state.markers.get(1) {
            Some(ObjectiveMarker::Shape(shape)) => {
                assert_eq!(shape.pos.x, 24.0);
                assert_eq!(shape.pos.y, 32.0);
                assert_eq!(shape.radius, 9.0);
            }
            other => panic!("expected shape marker, got {other:?}"),
        }
    }

    #[test]
    fn sync_throttles_to_three_ticks() {
        let mut world = World::new();
        world.insert_resource(LogicWorldState::new());
        let asm = Assembler::assemble("loop:\nop add x x 1\nsync x\njump loop always\n", true)
            .expect("assemble");
        let mut exec = Executor::new();
        exec.privileged = true;
        exec.load(asm);
        // `exec.build` is required for `sync`; use a synthesized building entity.
        let build = world
            .spawn(Building::new(TilePos::new(0, 0), BlockId::AIR, 0))
            .id();
        exec.build = Some(build);
        for tick in 0..13u64 {
            world
                .get_resource_mut::<LogicWorldState>()
                .expect("state")
                .tick = tick;
            exec.run(&mut world, 8);
        }
        let state = world.get_resource::<LogicWorldState>().expect("state");
        assert_eq!(state.count("sync"), 4);
    }
}
