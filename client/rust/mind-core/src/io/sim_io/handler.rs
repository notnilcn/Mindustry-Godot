// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SimIoHandler`, its live meta tags and the `MapSource` adapter.
//! Upstream: core/src/mindustry/io/SaveVersion.java (meta), core/src/mindustry/io/SaveIO.java

use std::path::Path;

use crate::content::{
    BlockKind, ContentError, ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content,
};
use crate::ecs::BuildingComp;
use crate::entities::comp::Building;
use crate::game::map_markers::MapMarkers;
use crate::game::runtime::CampaignRuntime;
use crate::io::fs::{FileSystem, NativeFs};
use crate::io::save::state::{MapSource, MarkersIo};
use crate::io::save::versions::v1::base_meta_tags;
use crate::io::save::{SaveIo, SaveOptions, WriteContext};
use crate::io::wire::WireWriter;
use crate::io::{IoResult, StringMap};
use crate::sim::unit_runtime::UnitRuntime;
use crate::sim::{IoHandler, IoRequest, IoResponse, IoStatus, Sim};
use crate::world::{EcsMapSource, WorldGrid};

use super::decode::decode_save_bytes;
use super::entities::LiveEntitySource;
use super::unit_codec::apply_team_plans;

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

/// The live `meta` tag set (`SaveVersion.writeMeta`, `SaveVersion.java:114-151`).
pub(super) fn live_meta_tags(
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
pub(super) struct SimMapSource<'a> {
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
        // `"mapname", state.map.name()` (`SaveVersion.java:133`): the loaded
        // map / sector / save name, not the port's `sim` placeholder (L02 T4,
        // PAR-0377: Load cards read "Map: Unknown").
        tags.insert("mapname".to_owned(), sim.map_name().to_owned());
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
        // `state.map = maps.byName(mapname) ?: new Map(name = mapname ?: "Unknown")`
        // (`SaveVersion.java:186-191`): either way the name is the saved
        // `mapname`.
        sim.set_map_name(save.tags.get("mapname").cloned().unwrap_or_default());
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
pub(super) fn boot_registry() -> Result<ContentRegistry, ContentError> {
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut content = create_base_content(&bundle, &store, true)?;
    content.init()?;
    content.post_init()?;
    Ok(content)
}
