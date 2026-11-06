// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Sector runtime + `SectorInfo` (plan 12 M3).
//!
//! Ported from `core/src/mindustry/type/Sector.java` (runtime half) and
//! `core/src/mindustry/game/SectorInfo.java`.
//!
//! The persisted JSON shape lives in [`crate::io::json::rules::SectorInfo`]
//! (plan 04). `SectorInfo` has three transient Arc fields (`counter`,
//! `means`, `loaded`) on every `ExportStat`; adding them to the plan-04 struct
//! would drop its `Copy` and pull `WindowedMean` into the IO layer, so plan 12
//! keeps the windows in the sibling [`InfoWindows`] and drives them through
//! [`SectorInfoState`] (documented deviation; the persisted shape is unchanged).

use indexmap::IndexMap;

use super::campaign_rules::CampaignRules;
use super::planet::Planet;
use super::rules::Rules;
use crate::content::{PlanetId, SectorId};
use crate::io::json::rules::{ExportStat, SectorInfo};
use crate::io::settings::SettingsStore;
use crate::math::WindowedMean;

pub use crate::io::json::content_serde::SectorKey;

/// `SectorInfo.valueWindow` (samples per rolling mean).
pub const VALUE_WINDOW: usize = 60;
/// `SectorInfo.refreshPeriod` = 60 ticks = 1 s (`SectorInfo.java:23`; Arc
/// `Interval.get` counts `Time.time` ticks, not seconds).
pub const REFRESH_PERIOD: f32 = 1.0;

/// Minimal core view needed by `SectorInfo::prepare`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CoreSnapshot {
    /// Items in the default team's core by name.
    pub items: IndexMap<String, i32>,
    /// `CoreBlock.storageCapacity`.
    pub storage_capacity: i32,
    /// Packed core spawn position (`Entity.pos()` truncation).
    pub spawn_position: i32,
    /// Whether a core entity exists.
    pub has_core: bool,
    /// Best available core block name.
    pub best_core_type: String,
    /// Whether the map has any enemy spawn points.
    pub has_spawns: bool,
    /// World width/height.
    pub width: i32,
    /// World height.
    pub height: i32,
    /// `lightCoverage` computed by the caller.
    pub light_coverage: f32,
    /// Default team has at least one `BlockFlag.launchPad` building.
    pub any_launch_pads: bool,
    /// Launch pads with `efficiency > 0` (`pads.contains(t -> t.efficiency > 0)`).
    pub active_launch_pads: usize,
    /// Known resources on the loaded map (`sector.info.resources`): the
    /// sorted, deduped [`scan_sector_resources`] of the tile grid.
    pub resources: Vec<String>,
}

/// `Block.isStatic()`: `cacheLayer == CacheLayer.walls` (`Block.java:1121`).
fn is_static_block(
    content: &crate::content::ContentRegistry,
    block: crate::content::BlockId,
) -> bool {
    content
        .block(block)
        .is_some_and(|def| def.cache_layer == crate::render::layer::CacheLayerId::Walls)
}

/// `World.getDarkness` for the sector resource scan (`World.java:484-529`).
///
/// `World.setSectorRules` clears `rules.limitMapArea` around the scan
/// (`World.java:311-313`), so the border term measures against the map edge;
/// the static-wall and generated-sector polygon terms are the grid's own.
fn sector_scan_darkness(
    grid: &crate::world::WorldGrid,
    content: &crate::content::ContentRegistry,
    x: i32,
    y: i32,
) -> f32 {
    crate::world::darkness::border_darkness(grid, grid.border_darkness, None, x, y)
        .max(crate::world::darkness::get_static_darkness(
            grid, content, x, y,
        ))
        .max(crate::world::darkness::sector_polygon_darkness(grid, x, y))
}

/// `World.setSectorRules`'s resource scan (`World.java:306-329`).
///
/// Every non-dark tile contributes its floor/overlay item drops and floor
/// liquid drop unless a static wall (ore-wall overlays included) covers it, plus
/// that wall's own drop (`Tile.wallDrop`). The result is the content-name list
/// Java stores in `sector.info.resources`, deduped (`ObjectSet`) and sorted by
/// `(ContentType, id)`: items first, then liquids, ids ascending
/// (`World.java:328`; `ContentType.item` ordinal 0, `liquid` ordinal 4).
///
/// Fixups (`Sector.loadInfo`, `Sector.java:105-118`): the list holds only item
/// and liquid names, so the port never stores the legacy `Blocks.water` entry
/// that Java remaps to `Liquids.water` — `water` is a liquid by construction —
/// and names that do not resolve to live content are dropped (Java's null
/// entries). Deterministic: no iteration-order dependence.
pub fn scan_sector_resources(
    grid: &crate::world::WorldGrid,
    content: &crate::content::ContentRegistry,
) -> Vec<String> {
    let mut items: Vec<u16> = Vec::new();
    let mut liquids: Vec<u16> = Vec::new();
    for y in 0..grid.tiles.height {
        for x in 0..grid.tiles.width {
            if sector_scan_darkness(grid, content, x, y) >= 3.0 {
                continue;
            }
            let tile = grid.tiles.get(x, y);
            if !is_static_block(content, tile.block) {
                if let Some(item) = content.block(tile.floor).and_then(|def| def.item_drop) {
                    items.push(item.raw());
                }
                if let Some(item) = content.block(tile.overlay).and_then(|def| def.item_drop) {
                    items.push(item.raw());
                }
                if let Some(liquid) = content.block(tile.floor).and_then(|def| def.liquid_drop) {
                    liquids.push(liquid.raw());
                }
            }
            if let Some(item) = tile.wall_drop(content) {
                items.push(item.raw());
            }
        }
    }
    items.sort_unstable();
    items.dedup();
    liquids.sort_unstable();
    liquids.dedup();
    items
        .into_iter()
        .filter_map(|raw| {
            content
                .item(crate::content::ItemId::new(raw))
                .map(|item| item.name.clone())
        })
        .chain(liquids.into_iter().filter_map(|raw| {
            content
                .liquid(crate::content::LiquidId::new(raw))
                .map(|liquid| liquid.name.clone())
        }))
        .collect()
}

/// `Sector.loadInfo`'s resource fixups (`Sector.java:105-118`) on a persisted
/// name list: duplicates collapse and empty entries (Java's deserialized `null`
/// content, `info.resources.contains(u -> u == null)`) are dropped. The legacy
/// `Blocks.water` → `Liquids.water` remap needs no port action: no block is
/// named `water`, so the name already resolves as the liquid.
pub fn fixup_resources(resources: &mut Vec<String>) {
    resources.retain(|name| !name.is_empty());
    let mut seen: Vec<String> = Vec::with_capacity(resources.len());
    resources.retain(|name| {
        if seen.contains(name) {
            return false;
        }
        seen.push(name.clone());
        true
    });
}

/// Builds the [`CoreSnapshot`] that `SectorInfo.prepare` consumes from the live
/// sim world (`SectorInfo.prepare(sector)`: the default team's first core, its
/// item module, capacity, spawn position, map dimensions and spawn count).
pub fn core_snapshot_of(
    sim: &crate::sim::Sim,
    content: &crate::content::ContentRegistry,
    default_team: u8,
) -> CoreSnapshot {
    let world = &sim.ecs.0;
    let mut snapshot = CoreSnapshot {
        best_core_type: "air".to_owned(),
        width: sim.grid.tiles.width,
        height: sim.grid.tiles.height,
        resources: scan_sector_resources(&sim.grid, content),
        ..CoreSnapshot::default()
    };
    snapshot.has_spawns = world
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .is_some_and(|runtime| runtime.session.spawn_count > 0);

    // `state.rules.defaultTeam.core()`: the first core of the default team in
    // entity order; `bestCoreType` is the largest core block (`TeamData`).
    let mut best_size = 0;
    let mut first_core: Option<bevy_ecs::entity::Entity> = None;
    for (_seq, entity, comp) in sim.ecs.entities_by_seq() {
        if comp.team.0 != default_team {
            continue;
        }
        let Some(def) = content.block(comp.block) else {
            continue;
        };
        // `SectorInfo.prepare`'s launch-pad query (`SectorInfo.java:244-249`):
        // any default-team launch pad, and the ones with `efficiency > 0`.
        if def
            .flags
            .contains(&crate::content::registries::blocks::BlockFlag::LaunchPad)
        {
            snapshot.any_launch_pads = true;
            let efficiency = world
                .get::<crate::entities::comp::Building>(entity)
                .map(|building| building.efficiency)
                .unwrap_or(0.0);
            if efficiency > 0.0 {
                snapshot.active_launch_pads += 1;
            }
        }
        if def.kind != crate::content::BlockKind::CoreBlock {
            continue;
        }
        if def.size > best_size {
            best_size = def.size;
            snapshot.best_core_type = def.name.clone();
        }
        if first_core.is_none() {
            first_core = Some(entity);
        }
    }
    if let Some(entity) = first_core {
        snapshot.has_core = true;
        if let Some(core) = world.get::<super::super::world::blocks::storage::CoreBuild>(entity) {
            snapshot.storage_capacity = core.storage_capacity;
        }
        // `SectorInfo.prepare` reads `defaultTeam.core().items`
        // (`SectorInfo.java:203-209`). Java's cores alias one module, so the
        // port sums the *canonical holders* of the default team's cores: every
        // aliased core (`mc/world/blocks/storage/team_items`) contributes its
        // team pool exactly once, never the inert per-core mirrors, and an
        // unaliased core contributes its own module (the old per-core sum).
        for (item, amount) in
            crate::world::blocks::storage::team_items::team_item_stacks(world, default_team)
        {
            if let Some(content_item) = content.item(item) {
                snapshot.items.insert(content_item.name.clone(), amount);
            }
        }
        if let Some(pos) = world.get::<crate::entities::comp::Pos>(entity) {
            let tile = (
                (pos.x / crate::content::TILE_SIZE) as i16,
                (pos.y / crate::content::TILE_SIZE) as i16,
            );
            snapshot.spawn_position = crate::world::TilePos::new(tile.0, tile.1).pack();
        }
    }
    snapshot
}

/// Transient statistic windows, parallel to [`SectorInfo`]'s persisted maps.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InfoWindows {
    /// Rolling windows keyed by item name.
    pub production: IndexMap<String, WindowedMean>,
    /// Raw production rolling windows keyed by item name.
    pub raw_production: IndexMap<String, WindowedMean>,
    /// Export rolling windows keyed by item name.
    pub export: IndexMap<String, WindowedMean>,
    /// Import rolling windows keyed by item name.
    pub imports: IndexMap<String, WindowedMean>,
    /// Core item input/output deltas by item index.
    pub core_deltas: Vec<i32>,
    /// Raw production deltas by item index.
    pub production_deltas: Vec<i32>,
    /// Cached import rates by item index.
    pub import_rate_cache: Vec<f32>,
    /// Accumulated refresh seconds (`Interval`).
    pub refresh_clock: f32,
}

impl InfoWindows {
    /// Adds a sample to a named window, creating it at `VALUE_WINDOW`.
    pub fn window<'a>(
        map: &'a mut IndexMap<String, WindowedMean>,
        name: &str,
    ) -> &'a mut WindowedMean {
        map.entry(name.to_owned())
            .or_insert_with(|| WindowedMean::new(VALUE_WINDOW))
    }

    fn update_stat_map(
        map: &mut IndexMap<String, ExportStat>,
        windows: &mut IndexMap<String, WindowedMean>,
    ) {
        // `SectorInfo.updateStats` — `mem::take` avoids borrowing both maps.
        let mut taken = std::mem::take(windows);
        for (name, stat) in map.iter_mut() {
            let window = taken
                .entry(name.clone())
                .or_insert_with(|| WindowedMean::new(VALUE_WINDOW));
            if !window.has_enough_data() && window.mean() == 0.0 {
                window.fill(stat.mean);
            }
            // `SectorInfo.java:297-309`: the window samples the transient
            // counter (items accumulated since the last refresh), then resets it.
            window.add(stat.counter.max(0.0));
            stat.counter = 0.0;
            stat.mean = window.raw_mean();
        }
        *windows = taken;
    }

    fn update_delta_map(
        map: &mut IndexMap<String, ExportStat>,
        windows: &mut IndexMap<String, WindowedMean>,
        deltas: &[i32],
        name_of: impl Fn(usize) -> Option<String>,
    ) {
        let mut taken = std::mem::take(windows);
        // `SectorInfo.updateDelta(item, map, deltas)` for every item: entries are
        // materialized on the first refresh, then fed the windowed deltas.
        for (index, delta) in deltas.iter().enumerate() {
            let Some(name) = name_of(index) else {
                continue;
            };
            let stat = map.entry(name.clone()).or_default();
            let window = taken
                .entry(name)
                .or_insert_with(|| WindowedMean::new(VALUE_WINDOW));
            if !window.has_enough_data() && window.mean() == 0.0 {
                window.fill(stat.mean);
            }
            window.add(*delta as f32);
            stat.mean = window.raw_mean();
        }
        *windows = taken;
    }
}

/// Persisted `SectorInfo` plus the transient statistic windows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SectorInfoState {
    /// Persisted half (`<planet>-s-<id>-info`).
    pub info: SectorInfo,
    /// Transient rolling windows.
    pub windows: InfoWindows,
    /// Temporary `lastImported` seq used by legacy launch pads.
    pub last_imported: IndexMap<String, i32>,
}

impl SectorInfoState {
    /// `SectorInfo` default (`new`).
    pub fn new() -> Self {
        Self::default()
    }

    /// `SectorInfo.handleCoreItem`.
    pub fn handle_core_item(&mut self, item_index: usize, amount: i32) {
        if self.windows.core_deltas.len() <= item_index {
            self.windows.core_deltas.resize(item_index + 1, 0);
        }
        self.windows.core_deltas[item_index] += amount;
    }

    /// `SectorInfo.handleProduction`.
    pub fn handle_production(&mut self, item_index: usize, amount: i32) {
        if self.windows.production_deltas.len() <= item_index {
            self.windows.production_deltas.resize(item_index + 1, 0);
        }
        self.windows.production_deltas[item_index] += amount;
    }

    /// `SectorInfo.handleItemExport`.
    pub fn handle_item_export(&mut self, name: &str, amount: i32) {
        let stat = self.info.export.entry(name.to_owned()).or_default();
        stat.counter += amount as f32;
    }

    /// `SectorInfo.handleItemImport`.
    pub fn handle_item_import(&mut self, name: &str, amount: i32) {
        let stat = self.info.imports.entry(name.to_owned()).or_default();
        stat.counter += amount as f32;
    }

    /// `SectorInfo.getExport`.
    pub fn get_export(&self, name: &str) -> f32 {
        self.info.export.get(name).map(|s| s.mean).unwrap_or(0.0)
    }

    /// `SectorInfo.hasExport`.
    pub fn has_export(&self, name: &str) -> bool {
        self.info
            .export
            .get(name)
            .is_some_and(|stat| stat.mean > 0.0)
    }

    /// `SectorInfo.exportRates`.
    pub fn export_rates(&self) -> IndexMap<String, f32> {
        self.info
            .export
            .iter()
            .map(|(name, stat)| (name.clone(), stat.mean))
            .collect()
    }

    /// `SectorInfo.anyExports`.
    pub fn any_exports(&self) -> bool {
        self.info.export.values().map(|s| s.mean).sum::<f32>() >= 0.01
    }

    /// `SectorInfo.refreshImportRates(planet)` given this sector's importers.
    ///
    /// `sources` are the `(item, mean)` pairs exported by every base that
    /// targets this sector.
    pub fn refresh_import_rates<'a>(
        &mut self,
        sources: impl IntoIterator<Item = (&'a str, f32)>,
        item_count: usize,
        index_of: impl Fn(&str) -> Option<usize>,
    ) {
        if self.windows.import_rate_cache.len() != item_count {
            self.windows.import_rate_cache = vec![0.0; item_count];
        } else {
            self.windows.import_rate_cache.fill(0.0);
        }
        for (name, mean) in sources {
            if let Some(index) = index_of(name)
                && index < self.windows.import_rate_cache.len()
            {
                self.windows.import_rate_cache[index] += mean;
            }
        }
    }

    /// `SectorInfo.getImportRates` (cached).
    pub fn get_import_rates(&self) -> &[f32] {
        &self.windows.import_rate_cache
    }

    /// `SectorInfo.getImportRate(planet, item)`.
    pub fn get_import_rate(&self, item_index: usize) -> f32 {
        self.windows
            .import_rate_cache
            .get(item_index)
            .copied()
            .unwrap_or(0.0)
    }

    /// `SectorInfo.updateStats` for the export/import maps.
    fn refresh_statistics(
        &mut self,
        index_of: &dyn Fn(&str) -> Option<usize>,
        name_of: &dyn Fn(usize) -> Option<String>,
    ) {
        InfoWindows::update_stat_map(&mut self.info.export, &mut self.windows.export);
        InfoWindows::update_stat_map(&mut self.info.imports, &mut self.windows.imports);

        if self.windows.core_deltas.is_empty() {
            self.windows.core_deltas = vec![0; self.windows.production_deltas.len().max(1)];
        }
        if self.windows.production_deltas.is_empty() {
            self.windows.production_deltas = vec![0; self.windows.core_deltas.len().max(1)];
        }

        // `production`/`rawProduction` are deltas, not counters.
        InfoWindows::update_delta_map(
            &mut self.info.production,
            &mut self.windows.production,
            &self.windows.core_deltas,
            name_of,
        );
        InfoWindows::update_delta_map(
            &mut self.info.raw_production,
            &mut self.windows.raw_production,
            &self.windows.production_deltas,
            name_of,
        );

        for (name, stat) in self.info.production.iter_mut() {
            let raw = self
                .info
                .raw_production
                .get(name)
                .map(|s| s.mean)
                .unwrap_or(0.0);
            stat.mean = stat.mean.min(raw);
        }
        for (name, stat) in self.info.export.iter_mut() {
            let raw = self
                .info
                .raw_production
                .get(name)
                .map(|s| s.mean)
                .unwrap_or(0.0);
            // raw + core draw
            let draw = self
                .info
                .production
                .get(name)
                .map(|p| (-p.mean).max(0.0))
                .unwrap_or(0.0);
            stat.mean = stat.mean.min(raw + draw);
        }
        let import_caps: Vec<(String, f32)> = self
            .info
            .imports
            .keys()
            .map(|name| {
                let max_rate = self
                    .get_import_rate(index_of(name).unwrap_or(usize::MAX))
                    .max(0.0);
                (name.clone(), max_rate)
            })
            .collect();
        for (name, max_rate) in import_caps {
            if let Some(stat) = self.info.imports.get_mut(&name) {
                stat.mean = stat.mean.min(max_rate);
            }
        }

        self.windows.core_deltas.fill(0);
        self.windows.production_deltas.fill(0);
    }

    /// `SectorInfo.update()` (60-tick refresh; client gate is the caller's).
    ///
    /// `item_count` is the live content item count: `updateDelta` materializes a
    /// `production`/`rawProduction` entry for every item on each refresh.
    pub fn update(
        &mut self,
        seconds_delta: f32,
        item_index_of: &dyn Fn(&str) -> Option<usize>,
        item_name_of: &dyn Fn(usize) -> Option<String>,
        item_count: usize,
    ) {
        self.windows.refresh_clock += seconds_delta;
        // Half a tick of tolerance: callers feed `1/60` per tick and 60 f32
        // additions round to slightly under `1.0` (0.9999997), while Java's
        // integer-tick `Interval.get(60)` fires exactly on the 60th tick.
        if self.windows.refresh_clock < REFRESH_PERIOD - 0.5 / 60.0 {
            return;
        }
        self.windows.refresh_clock %= REFRESH_PERIOD;
        self.windows.core_deltas.resize(item_count.max(1), 0);
        self.windows.production_deltas.resize(item_count.max(1), 0);
        self.refresh_statistics(item_index_of, item_name_of);
    }

    /// `SectorInfo.prepare(sector)`.
    pub fn prepare(
        &mut self,
        core: &CoreSnapshot,
        min_production: bool,
        preset_name: Option<&str>,
    ) {
        self.info.items.clear();
        for (name, amount) in &core.items {
            self.info.items.insert(name.clone(), *amount);
        }
        if self.info.spawn_position == 0 {
            self.info.spawn_position = core.spawn_position;
        }
        self.info.has_core = core.has_core;
        self.info.best_core_type = if core.has_core {
            core.best_core_type.clone()
        } else {
            "air".to_owned()
        };
        self.info.storage_capacity = core.storage_capacity;
        self.info.has_spawns = core.has_spawns;
        self.info.last_width = core.width;
        self.info.last_height = core.height;
        self.info.light_coverage = core.light_coverage;
        // `World.setSectorRules` (`World.java:328`): the loaded map's known
        // resources replace the record's list on every prepare (`SectorInfo`
        // is written with them, so the planet page's `@sectors.resources` row
        // has content).
        self.info.resources = core.resources.clone();
        // `SectorInfo.java:226-228` + `:244-249`.
        self.info.last_preset_name = preset_name.map(str::to_owned);
        if core.active_launch_pads == 0 {
            self.info.export.clear();
        }

        if min_production {
            // cap production at raw production
            for (name, stat) in self.info.production.iter_mut() {
                let raw = self
                    .info
                    .raw_production
                    .get(name)
                    .map(|s| s.mean)
                    .unwrap_or(0.0);
                stat.mean = stat.mean.min(raw);
            }
        }
    }

    /// `SectorInfo.write()` (`SectorInfo.java:165-196`): record -> live session.
    ///
    /// Returns the item stacks to put into the default core (clamped by the
    /// caller against the core's `storageCapacity`).
    pub fn write_to_state(
        &mut self,
        wave_team_has_core: bool,
        planet_allow_waves: bool,
        preset_capture_wave: i32,
        session: &mut crate::game::play::PlaySession,
    ) -> IndexMap<String, i32> {
        if wave_team_has_core {
            self.info.attack = true;
            if !planet_allow_waves {
                self.info.win_wave = 0;
            }
        }
        if self.info.win_wave <= 0 && !self.info.attack && planet_allow_waves {
            self.info.win_wave = 30;
        }
        if preset_capture_wave > 0 && !planet_allow_waves {
            self.info.win_wave = preset_capture_wave;
        }
        session.wave = self.info.wave;
        session.rules.waves = self.info.waves;
        session.rules.wave_spacing = self.info.wave_spacing;
        session.rules.win_wave = self.info.win_wave;
        session.rules.attack_mode = self.info.attack;
        self.info.items.clone()
    }

    /// `SectorInfo.handleItemExport(ItemStack)`.
    pub fn handle_item_export_stack(&mut self, name: &str, amount: i32) {
        self.handle_item_export(name, amount);
    }
}

/// Settings key `<planet>-s-<id>-info` (free form for external writers).
pub fn info_key(planet_name: &str, sector_id: u16) -> String {
    format!("{planet_name}-s-{sector_id}-info")
}

/// Sector runtime (`type/Sector.java`, runtime half).
#[derive(Debug, Clone, PartialEq)]
pub struct Sector {
    /// Owning planet.
    pub planet: PlanetId,
    /// Dense sector index in the planet grid.
    pub id: u16,
    /// Assigned preset, if any.
    pub preset: Option<SectorId>,
    /// Preset difficulty (copied from content).
    pub preset_difficulty: f32,
    /// Preset `requireUnlock`.
    pub preset_require_unlock: bool,
    /// Preset `captureWave`.
    pub preset_capture_wave: i32,
    /// Preset `alwaysUnlocked`.
    pub preset_always_unlocked: bool,
    /// Preset name (for `sectorDataMatches`/save info).
    pub preset_name: Option<String>,
    /// Preset `startWaveTimeMultiplier`.
    pub preset_start_wave_time_multiplier: f32,
    /// Preset `addStartingItems`.
    pub preset_add_starting_items: bool,
    /// Preset `noLighting`.
    pub preset_no_lighting: bool,
    /// Preset `isLastSector`.
    pub preset_is_last_sector: bool,
    /// Preset `attackAfterWaves`.
    pub preset_attack_after_waves: bool,
    /// Preset `overrideLaunchDefaults`.
    pub preset_override_launch_defaults: bool,
    /// Preset `allowLaunchLoadout`.
    pub preset_allow_launch_loadout: bool,
    /// Preset `allowLaunchSchematics`.
    pub preset_allow_launch_schematics: bool,
    /// Visual shield target sector index.
    pub shield_target: Option<u16>,
    /// Guarded preset's `shieldSectors` (`SectorPreset.java:46`): this sector
    /// stays shielded while any listed sector is uncaptured.
    pub shield_sectors: Vec<u16>,
    /// Difficulty based on nearby bases.
    pub threat: f32,
    /// Generate an enemy base here.
    pub generate_enemy_base: bool,
    /// Save slot name when the sector has a base (`None` = no save).
    pub save: Option<String>,
    /// The fixed-step game is currently playing this sector.
    pub being_played: bool,
    /// Persisted + transient stats.
    pub info: SectorInfoState,
}

impl Sector {
    /// Creates an unassigned sector for `planet`.
    pub fn new(planet: PlanetId, id: u16) -> Self {
        Self {
            planet,
            id,
            preset: None,
            preset_difficulty: 0.0,
            preset_require_unlock: true,
            preset_capture_wave: 0,
            preset_always_unlocked: false,
            preset_name: None,
            preset_start_wave_time_multiplier: 2.0,
            preset_add_starting_items: false,
            preset_no_lighting: false,
            preset_is_last_sector: false,
            preset_attack_after_waves: false,
            preset_override_launch_defaults: false,
            preset_allow_launch_loadout: false,
            preset_allow_launch_schematics: false,
            shield_target: None,
            shield_sectors: Vec::new(),
            threat: 0.0,
            generate_enemy_base: false,
            save: None,
            being_played: false,
            info: SectorInfoState::new(),
        }
    }

    /// Settings key `<planet>-s-<id>-info`.
    pub fn info_key(&self, planet_name: &str) -> String {
        info_key(planet_name, self.id)
    }

    /// `Sector.loadInfo`.
    pub fn load_info(&mut self, settings: &SettingsStore, planet_name: &str) {
        self.info.info = settings.get_json_or(&self.info_key(planet_name), SectorInfo::default);
        fixup_resources(&mut self.info.info.resources);
    }

    /// `Sector.saveInfo`.
    pub fn save_info(
        &self,
        settings: &mut SettingsStore,
        planet_name: &str,
    ) -> Result<(), crate::io::IoError> {
        settings.put_json(&self.info_key(planet_name), &self.info.info)
    }

    /// `Sector.clearInfo`.
    pub fn clear_info(&mut self, settings: &mut SettingsStore, planet_name: &str) {
        self.info = SectorInfoState::new();
        settings.remove(&self.info_key(planet_name));
    }

    /// `Sector.unlocked`.
    pub fn unlocked(&self) -> bool {
        self.has_base() || self.preset_always_unlocked
    }

    /// `Sector.locked`.
    pub fn locked(&self) -> bool {
        !self.unlocked()
    }

    /// `Sector.hasBase` (`state.gameOver` handled by the live-game caller).
    pub fn has_base(&self) -> bool {
        self.save.is_some() && self.info.info.has_core
    }

    /// `Sector.hasSave`.
    pub fn has_save(&self) -> bool {
        self.save.is_some()
    }

    /// `Sector.isBeingPlayed`.
    pub fn is_being_played(&self) -> bool {
        self.being_played
    }

    /// `Sector.isAttacked` (`playing_rules` = `(waves, attackMode)` when this
    /// sector is the one being played).
    pub fn is_attacked(&self, playing: Option<(bool, bool)>) -> bool {
        if self.being_played {
            return playing.map(|(w, a)| w || a).unwrap_or(false);
        }
        self.save.is_some()
            && (self.info.info.waves || self.info.info.attack)
            && self.info.info.has_core
    }

    /// `Sector.isFrozen`.
    pub fn is_frozen(&self, playing: Option<(bool, bool)>) -> bool {
        self.is_attacked(playing) && !self.is_being_played()
    }

    /// `Sector.isCaptured`.
    pub fn is_captured(&self, playing: Option<(bool, bool)>) -> bool {
        if self.being_played {
            return playing.map(|(w, a)| !w && !a).unwrap_or(false);
        }
        self.save.is_some() && !self.info.info.waves && !self.info.info.attack
    }

    /// `Sector.hasEnemyBase`.
    pub fn has_enemy_base(&self) -> bool {
        let generated = (self.generate_enemy_base && self.preset.is_none())
            || (self.preset.is_some() && self.preset_capture_wave == 0);
        generated && (self.save.is_none() || self.info.info.attack || !self.has_base())
    }

    /// `Sector.isLastSector` (`preset.isLastSector`).
    pub fn is_last_sector(&self) -> bool {
        self.preset_is_last_sector
    }

    /// `Sector.allowLaunchLoadout()`.
    pub fn allow_launch_loadout(&self, planet_allow: bool) -> bool {
        if self.preset.is_some() && self.preset_override_launch_defaults {
            self.preset_allow_launch_loadout
        } else {
            planet_allow
        }
    }

    /// `Sector.allowLaunchSchematics()`.
    pub fn allow_launch_schematics(&self, planet_allow: bool) -> bool {
        if self.preset.is_some() && self.preset_override_launch_defaults {
            self.preset_allow_launch_schematics
        } else {
            planet_allow
        }
    }

    /// `Sector.isShielded` (`Sector.java:125-127`): the guarded preset's
    /// `shieldSectors` list is non-empty, this sector is uncaptured and at
    /// least one listed sector is still uncaptured. The reverse
    /// `shieldTarget` link is only the `PlanetDialog` arc source, not the gate.
    pub fn is_shielded(&self, planet: &Planet) -> bool {
        !self.shield_sectors.is_empty()
            && !self.is_captured(None)
            && self.shield_sectors.iter().any(|id| {
                planet
                    .sector(*id)
                    .is_some_and(|sector| !sector.is_captured(None))
            })
    }

    /// `Sector.displayThreat` (threat bucket; color is the caller's).
    ///
    /// `Sector.java:84` special-cases exactly `SectorDifficulty.unreasonable`
    /// (13, "special difficulty reserved for 27"), not `eradication` (10), and
    /// otherwise reads `threats[(int)(threat / 0.25)]`.
    pub fn threat_bucket(&self) -> &'static str {
        const THREATS: [&str; 5] = ["low", "medium", "high", "extreme", "eradication"];
        const UNREASONABLE: f32 =
            crate::content::registries::sectors::sector_difficulty::UNREASONABLE as f32;
        if (self.preset_difficulty - UNREASONABLE).abs() < 0.000_001 {
            return "unreasonable";
        }
        let index = ((self.threat / 0.25) as usize).min(THREATS.len() - 1);
        THREATS[index]
    }

    /// `Sector.save_info` after `SectorInfo::prepare` (both key + payload).
    pub fn sector_data_matches(&self, preset_name: Option<&str>, width: i32, height: i32) -> bool {
        if preset_name.is_some()
            && (self.info.info.last_width != width || self.info.info.last_height != height)
        {
            return false;
        }
        self.preset_name.as_deref() == preset_name
            && self.info.info.last_preset_name.as_deref() == preset_name
    }

    /// `Sector.addItems(ItemSeq)`; caps at `storageCapacity`, clamps >= 0.
    pub fn add_items(&mut self, items: impl IntoIterator<Item = (String, i32)>) {
        if !self.has_base() {
            return;
        }
        let capacity = self.info.info.storage_capacity;
        for (name, amount) in items {
            let slot = self.info.info.items.entry(name).or_insert(0);
            *slot = (*slot + amount).min(capacity).max(0);
        }
    }

    /// `Sector.items()` snapshot.
    pub fn items(&self) -> &IndexMap<String, i32> {
        &self.info.info.items
    }

    /// `Sector.removeItems(ItemSeq)`.
    pub fn remove_items(&mut self, items: impl IntoIterator<Item = (String, i32)>) {
        self.add_items(items.into_iter().map(|(name, amount)| (name, -amount)));
    }

    /// Applies campaign rules at load (`Planet.applyRules` sector-half).
    pub fn apply_rules(
        &self,
        campaign: &CampaignRules,
        rules: &mut Rules,
        planet: &crate::content::registries::planets::PlanetDef,
        is_game: bool,
    ) -> super::campaign_rules::CampaignApply {
        campaign.apply(planet, rules, is_game)
    }
}

/// Packs a `(x, y)` tile position the way Arc `Point2.pack` does
/// (`(x << 16) | y & 0xffff`; x is the high half).
pub const fn pack_position(x: i32, y: i32) -> i32 {
    (x << 16) | (y & 0xffff)
}

/// Unpacks a packed position.
pub const fn unpack_position(packed: i32) -> (i32, i32) {
    (packed >> 16, (packed as u16 as i16) as i32)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;
    use crate::game::play::PlaySession;

    #[test]
    fn info_windows_update_and_cap() {
        let mut state = SectorInfoState::new();
        // Seed export/import counters then refresh.
        state.handle_item_export("silicon", 30);
        state.handle_item_import("lead", 10);
        // `SectorInfo.java:123-130`: `handleItemExport` fills the transient
        // counter; the mean only moves when `updateStats` refreshes.
        assert_eq!(state.info.export["silicon"].counter, 30.0);
        assert_eq!(state.get_export("silicon"), 0.0);
        assert_eq!(state.info.imports["lead"].counter, 10.0);

        // `handleItemExport` is a counter; updateStats moves it through the window.
        state
            .info
            .raw_production
            .entry("silicon".to_owned())
            .or_default()
            .mean = 5.0;
        state.windows.refresh_clock = REFRESH_PERIOD;
        state.update(
            0.0,
            &|name| match name {
                "silicon" => Some(0),
                "lead" => Some(1),
                _ => None,
            },
            &|index| match index {
                0 => Some("silicon".to_owned()),
                1 => Some("lead".to_owned()),
                _ => None,
            },
            2,
        );
        // Export was capped by raw production + core draw (draw is 0).
        assert!(state.get_export("silicon") <= 5.0 + 1e-3);
    }

    #[test]
    fn export_counter_decays() {
        let mut state = SectorInfoState::new();
        // `updateStats` caps the export mean to `rawProduction + core draw`
        // (`SectorInfo.java:330-340`), so the raw side must exist for the
        // sample to survive; the window still holds 100 / 60.
        state
            .info
            .raw_production
            .entry("copper".to_owned())
            .or_default()
            .mean = 5.0;
        state.handle_item_export("copper", 100);
        state.update(1.0, &|_| None, &|_| None, 0);
        // One sample in a 60-slot window (`SectorInfo.java:297-309`).
        assert_eq!(state.get_export("copper"), 100.0 / 60.0);
        // A refresh with no new exports keeps the window sample and resets the
        // counter.
        state.update(1.0, &|_| None, &|_| None, 0);
        assert_eq!(state.get_export("copper"), 100.0 / 60.0);
        assert_eq!(state.info.export["copper"].counter, 0.0);
    }

    #[test]
    fn refresh_every_60_ticks() {
        let mut state = SectorInfoState::new();
        state.windows.production_deltas = vec![1; 1];
        for _ in 0..59 {
            state.update(
                1.0 / 60.0,
                &|_| None,
                &|index| (index == 0).then(|| "copper".to_owned()),
                1,
            );
        }
        assert!(
            state.info.production.is_empty(),
            "no refresh before 60 ticks (1 s)"
        );
        state.update(
            1.0 / 60.0,
            &|_| None,
            &|index| (index == 0).then(|| "copper".to_owned()),
            1,
        );
        assert!(
            state.info.production.contains_key("copper"),
            "the 60th tick refreshes every item's statistics"
        );
    }

    #[test]
    fn sector_base_attack_capture_semantics() {
        let mut sector = Sector::new(PlanetId::new(5), 15);
        assert!(!sector.has_base());
        assert!(!sector.has_enemy_base());

        sector.save = Some("sector-serpulo-15".to_owned());
        sector.info.info.has_core = true;
        // `SectorInfo.waves` defaults to true; a freshly captured/owned sector
        // has waves and attack mode turned off by `SectorInfo::write`.
        sector.info.info.waves = false;
        sector.info.info.attack = false;
        assert!(sector.has_base());
        assert!(sector.is_captured(None), "waves/attack false -> captured");

        sector.info.info.attack = true;
        assert!(sector.is_attacked(None));
        assert!(sector.is_frozen(None));
        assert!(!sector.is_captured(None));
    }

    #[test]
    fn add_items_caps_and_clamps() {
        let mut sector = Sector::new(PlanetId::new(5), 1);
        sector.save = Some("save".to_owned());
        sector.info.info.has_core = true;
        sector.info.info.storage_capacity = 100;
        sector.add_items([("copper".to_owned(), 60)]);
        sector.add_items([("copper".to_owned(), 90)]);
        assert_eq!(sector.items().get("copper"), Some(&100));
        sector.remove_items([("copper".to_owned(), 1000)]);
        assert_eq!(sector.items().get("copper"), Some(&0));
    }

    #[test]
    fn prepare_and_write_populate_fields() {
        let mut state = SectorInfoState::new();
        let core = CoreSnapshot {
            items: IndexMap::from([("copper".to_owned(), 250)]),
            storage_capacity: 4000,
            spawn_position: pack_position(32, 48),
            has_core: true,
            best_core_type: "core-nucleus".to_owned(),
            has_spawns: true,
            width: 200,
            height: 200,
            light_coverage: 3.5,
            any_launch_pads: true,
            active_launch_pads: 1,
            resources: vec!["copper".to_owned(), "water".to_owned()],
        };
        state.prepare(&core, false, Some("groundZero"));
        assert_eq!(state.info.items.get("copper"), Some(&250));
        assert_eq!(state.info.storage_capacity, 4000);
        assert_eq!(state.info.spawn_position, pack_position(32, 48));
        assert_eq!(unpack_position(state.info.spawn_position), (32, 48));
        assert_eq!(state.info.light_coverage, 3.5);
        assert_eq!(state.info.last_preset_name.as_deref(), Some("groundZero"));
        assert_eq!(
            state.info.resources,
            vec!["copper".to_owned(), "water".to_owned()],
            "prepare writes the scanned resources (`World.java:328`)"
        );

        state.info.wave = 7;
        state.info.win_wave = -1;
        state.info.wave_spacing = 7200.0;
        state.info.waves = true;
        state.info.attack = false;
        let mut session = PlaySession::default();
        let items = state.write_to_state(false, true, 0, &mut session);
        assert_eq!(items.get("copper"), Some(&250));
        assert_eq!(session.wave, 7);
        assert_eq!(
            session.rules.win_wave, 30,
            "infinite waves get a default win wave"
        );
        assert!(!session.rules.attack_mode);
    }

    #[test]
    fn write_to_state_applies_record_and_win_wave() {
        let mut state = SectorInfoState::new();
        state.info.wave = 7;
        state.info.waves = true;
        state.info.win_wave = 0;
        state.info.attack = false;
        state.info.items.insert("copper".to_owned(), 5000);
        let mut session = PlaySession::default();
        // No wave core: the `winWave <= 0 && !attack && allowWaves` branch fills
        // Java's 30 default (`SectorInfo.java:168`).
        let items = state.write_to_state(false, true, 0, &mut session);
        assert_eq!(items.get("copper"), Some(&5000));
        assert_eq!(session.wave, 7);
        assert!(!session.rules.attack_mode);
        assert_eq!(session.rules.win_wave, 30);

        // Same record with a wave-team core (the first write set winWave 30, so
        // it survives; attack mode flips on).
        let mut session = PlaySession::default();
        state.write_to_state(true, true, 0, &mut session);
        assert!(session.rules.attack_mode, "a wave-team core forces attack");
        assert_eq!(
            session.rules.win_wave, 30,
            "allowWaves keeps the 30 default (`SectorInfo.java:168`)"
        );

        // `planet.allowWaves == false`: a wave core zeroes the win wave.
        let mut state = SectorInfoState::new();
        state.info.attack = false;
        state.info.win_wave = 12;
        let mut session = PlaySession::default();
        state.write_to_state(true, false, 0, &mut session);
        assert_eq!(session.rules.win_wave, 0);
        assert!(session.rules.attack_mode);
        // No core + a preset capture wave replaces the default.
        let mut state = SectorInfoState::new();
        state.info.win_wave = 12;
        let mut session = PlaySession::default();
        state.write_to_state(false, false, 42, &mut session);
        assert_eq!(session.rules.win_wave, 42);
    }

    #[test]
    fn prepare_sets_last_preset_and_clears_export_without_pads() {
        let mut state = SectorInfoState::new();
        state.info.export.insert(
            "copper".to_owned(),
            ExportStat {
                mean: 3.0,
                ..Default::default()
            },
        );
        let core = CoreSnapshot {
            has_core: true,
            active_launch_pads: 0,
            ..CoreSnapshot::default()
        };
        state.prepare(&core, false, Some("groundZero"));
        assert_eq!(state.info.last_preset_name.as_deref(), Some("groundZero"));
        assert!(
            state.info.export.is_empty(),
            "no active launch pad clears the stored export (`SectorInfo.java:244-249`)"
        );
        // With an active pad the export survives.
        let mut state = SectorInfoState::new();
        state.info.export.insert(
            "copper".to_owned(),
            ExportStat {
                mean: 3.0,
                ..Default::default()
            },
        );
        let core = CoreSnapshot {
            has_core: true,
            any_launch_pads: true,
            active_launch_pads: 1,
            ..CoreSnapshot::default()
        };
        state.prepare(&core, false, None);
        assert_eq!(state.info.export.get("copper").map(|s| s.mean), Some(3.0));
        assert_eq!(state.info.last_preset_name, None);
    }

    /// CG-SECTOR-03: `World.setSectorRules`'s tile resource scan
    /// (`World.java:306-329`): floor/overlay/wall drops + floor liquids on
    /// non-dark tiles only, deduped and sorted (items by id, then liquids),
    /// with the static-wall suppression rules.
    #[test]
    fn scan_sector_resources_matches_set_sector_rules() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};

        let content = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("content");
        let id = |name: &str| content.block_id(name).expect("block");
        let mut grid = crate::world::WorldGrid::new(8, 8);
        // Isolate the static-wall darkness term: the border term would darken
        // the outer two rings of an 8x8 map (`World.java:487-503`).
        grid.border_darkness = false;

        // Ore overlay on buildable ground -> `tile.overlay().itemDrop`.
        grid.tiles.get_mut(2, 2).overlay = id("ore-copper");
        // A second copper tile must not duplicate the entry (`ObjectSet`).
        grid.tiles.get_mut(2, 3).floor = id("stone");
        grid.tiles.get_mut(2, 3).overlay = id("ore-copper");
        // Floor drop -> `tile.floor().itemDrop`.
        grid.tiles.get_mut(3, 3).floor = id("sand-floor");
        // Wall ore -> `tile.wallDrop()` regardless of the floor.
        grid.tiles.get_mut(4, 4).block = id("ore-wall-thorium");
        // Floor liquid -> `tile.floor().liquidDrop`.
        grid.tiles.get_mut(5, 5).floor = id("deep-water");
        // A non-static building does not suppress the drops under it.
        grid.tiles.get_mut(1, 6).floor = id("stone");
        grid.tiles.get_mut(1, 6).overlay = id("ore-lead");
        grid.tiles.get_mut(1, 6).block = id("conveyor");
        // A static wall suppresses the floor/overlay/liquid drops beneath it...
        grid.tiles.get_mut(6, 6).floor = id("sand-floor");
        grid.tiles.get_mut(6, 6).block = id("sand-wall");
        grid.tiles.get_mut(6, 7).floor = id("deep-water");
        grid.tiles.get_mut(6, 7).block = id("sand-wall");
        // ...and a dark (>= 3) static tile is skipped entirely.
        grid.tiles.get_mut(0, 0).floor = id("sand-floor");
        grid.tiles.get_mut(0, 0).block = id("sand-wall");
        grid.tiles.get_mut(0, 0).data = 4;

        assert_eq!(
            scan_sector_resources(&grid, &content),
            vec![
                "copper".to_owned(),
                "lead".to_owned(),
                "sand".to_owned(),
                "thorium".to_owned(),
                "water".to_owned(),
            ],
            "items sort by id before liquids (`World.java:328`)"
        );
    }

    /// `Sector.loadInfo`'s fixups (`Sector.java:105-118`) on a persisted list:
    /// duplicates collapse and empty entries drop; `water` stays a liquid.
    #[test]
    fn load_info_fixes_up_resources() {
        let mut settings = SettingsStore::new();
        let mut sector = Sector::new(PlanetId::new(5), 3);
        sector.info.info.resources = vec![
            "copper".to_owned(),
            String::new(),
            "copper".to_owned(),
            "water".to_owned(),
        ];
        sector.save_info(&mut settings, "serpulo").unwrap();
        let mut loaded = Sector::new(PlanetId::new(5), 3);
        loaded.load_info(&settings, "serpulo");
        assert_eq!(
            loaded.info.info.resources,
            vec!["copper".to_owned(), "water".to_owned()]
        );
    }

    #[test]
    fn settings_roundtrip() {
        let mut settings = SettingsStore::new();
        let mut sector = Sector::new(PlanetId::new(5), 12);
        sector.info.info.wave = 9;
        sector.info.info.items.insert("lead".to_owned(), 50);
        sector.save_info(&mut settings, "serpulo").unwrap();
        assert!(settings.has(&sector.info_key("serpulo")));

        let mut loaded = Sector::new(PlanetId::new(5), 12);
        loaded.load_info(&settings, "serpulo");
        assert_eq!(loaded.info.info.wave, 9);
        assert_eq!(loaded.info.info.items.get("lead"), Some(&50));
    }

    /// GAPS #21: `SectorInfo.prepare` reads the default team's core module
    /// (`SectorInfo.java:203-209`). Two cores of one team alias one pool
    /// (`CoreBlock.java:764-770`), so the snapshot must carry the shared total
    /// once — not the first core's module and not the sum of duplicate copies.
    #[test]
    fn core_snapshot_sums_the_team_pool() {
        use crate::command::Command;
        use crate::content::BlockId;
        use crate::sim::Sim;
        use crate::world::blocks::storage::team_items;

        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        let (core, copper) = {
            let content = sim.block_runtime().expect("runtime").content();
            (
                content.block_id("core-shard").expect("core-shard"),
                content.item_id("copper").expect("copper"),
            )
        };
        for (x, y) in [(6, 6), (26, 26)] {
            sim.apply(Command::Place { x, y, block: core })
                .expect("place core");
        }
        // `BuildRules.default_team` of the bare runtime.
        let team = 0u8;
        let cores = team_items::team_cores(&sim.ecs.0, team);
        assert_eq!(cores.len(), 2, "two default-team cores");
        let (first, second) = (cores[0], cores[1]);
        assert_eq!(team_items::core_holder(&sim.ecs.0, second), first);

        // Deposit through the *second* core: both resolve to one pool.
        team_items::add_to_core(&mut sim.ecs.0, second, copper, 100, 8000);

        let snapshot = {
            let content = sim.block_runtime().expect("runtime").content();
            core_snapshot_of(&sim, content, team)
        };
        assert!(snapshot.has_core);
        assert_eq!(snapshot.best_core_type, "core-shard");
        assert_eq!(
            snapshot.items.get("copper"),
            Some(&100),
            "one pool, counted once"
        );
        assert_eq!(snapshot.storage_capacity, 8000, "two core-shard capacities");
        assert_eq!(snapshot.spawn_position, pack_position(6, 6));

        // The sector record keeps the total (`SectorInfo.prepare`).
        let mut state = SectorInfoState::new();
        state.prepare(&snapshot, false, Some("groundZero"));
        assert_eq!(state.info.items.get("copper"), Some(&100));
        assert_eq!(state.info.storage_capacity, 8000);
        assert_eq!(state.info.best_core_type, "core-shard");
    }

    /// PAR-0453: `Sector.displayThreat` (`Sector.java:80-84`) special-cases
    /// `SectorDifficulty.unreasonable` (13, "special difficulty reserved for
    /// 27"), NOT `eradication` (10); every other sector falls through to
    /// `threats[(int)(threat / 0.25)]`.
    #[test]
    fn threat_bucket_uses_unreasonable_difficulty() {
        let mut sector = Sector::new(PlanetId::new(5), 27);
        sector.preset_difficulty = 13.0; // SectorDifficulty.unreasonable
        sector.threat = 0.0;
        assert_eq!(sector.threat_bucket(), "unreasonable");

        // 10 is eradication: the label comes from the threat value instead.
        sector.preset_difficulty = 10.0;
        sector.threat = 0.0;
        assert_eq!(sector.threat_bucket(), "low");
        sector.threat = 1.0;
        assert_eq!(sector.threat_bucket(), "eradication");

        // No preset (0.0) and a normal first sector.
        sector.preset_difficulty = 0.0;
        sector.threat = 0.3;
        assert_eq!(sector.threat_bucket(), "medium");
    }
}
