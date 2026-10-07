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
use super::rules::Rules;
use crate::content::{PlanetId, SectorId};
use crate::io::json::rules::{ExportStat, SectorInfo};
use crate::io::settings::SettingsStore;
use crate::math::WindowedMean;

pub use crate::io::json::content_serde::SectorKey;

/// `SectorInfo.valueWindow` (samples per rolling mean).
pub const VALUE_WINDOW: usize = 60;
/// `SectorInfo.refreshPeriod` (seconds between statistic refreshes).
pub const REFRESH_PERIOD: f32 = 60.0;

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
}

/// Minimal sector/world view needed by `SectorInfo::write`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WriteTarget {
    /// `rules.waveTeam` has a core.
    pub wave_team_has_core: bool,
    /// `planet.allowWaves`.
    pub planet_allow_waves: bool,
    /// `sector.preset.captureWave`.
    pub preset_capture_wave: i32,
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
            let sample = stat.mean;
            window.add(sample.max(0.0));
            stat.mean = window.raw_mean();
        }
        *windows = taken;
    }

    fn update_delta_map(
        map: &mut IndexMap<String, ExportStat>,
        windows: &mut IndexMap<String, WindowedMean>,
        deltas: &mut [i32],
        index_of: impl Fn(&str) -> Option<usize>,
    ) {
        let mut taken = std::mem::take(windows);
        for (name, stat) in map.iter_mut() {
            let window = taken
                .entry(name.clone())
                .or_insert_with(|| WindowedMean::new(VALUE_WINDOW));
            if !window.has_enough_data() && window.mean() == 0.0 {
                window.fill(stat.mean);
            }
            let delta = index_of(name)
                .and_then(|i| deltas.get(i).copied())
                .unwrap_or(0);
            window.add(delta as f32);
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
        stat.mean += amount as f32;
    }

    /// `SectorInfo.handleItemImport`.
    pub fn handle_item_import(&mut self, name: &str, amount: i32) {
        let stat = self.info.imports.entry(name.to_owned()).or_default();
        stat.mean += amount as f32;
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
    fn refresh_statistics(&mut self, index_of: &dyn Fn(&str) -> Option<usize>) {
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
            &mut self.windows.core_deltas,
            |name| index_of(name),
        );
        InfoWindows::update_delta_map(
            &mut self.info.raw_production,
            &mut self.windows.raw_production,
            &mut self.windows.production_deltas,
            |name| index_of(name),
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
    pub fn update(&mut self, seconds_delta: f32, item_index_of: &dyn Fn(&str) -> Option<usize>) {
        self.windows.refresh_clock += seconds_delta;
        if self.windows.refresh_clock < REFRESH_PERIOD {
            return;
        }
        self.windows.refresh_clock %= REFRESH_PERIOD;
        self.refresh_statistics(item_index_of);
    }

    /// `SectorInfo.prepare(sector)`.
    pub fn prepare(&mut self, core: &CoreSnapshot, min_production: bool) {
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

    /// `SectorInfo.write()`.
    pub fn write(
        &mut self,
        target: &WriteTarget,
        wave: i32,
        win_wave: i32,
        wave_spacing: f32,
        waves: bool,
        attack: bool,
    ) {
        let mut win = win_wave;
        let mut attack_mode = attack;
        if target.wave_team_has_core {
            attack_mode = true;
            if !target.planet_allow_waves {
                win = 0;
            }
        }
        if win <= 0 && !attack_mode && target.planet_allow_waves {
            win = 30;
        }
        if target.preset_capture_wave > 0 && !target.planet_allow_waves {
            win = target.preset_capture_wave;
        }
        self.info.wave = wave;
        self.info.win_wave = win;
        self.info.wave_spacing = wave_spacing;
        self.info.waves = waves;
        self.info.attack = attack_mode;
    }

    /// `SectorInfo.handleItemExport(ItemStack)`.
    pub fn handle_item_export_stack(&mut self, name: &str, amount: i32) {
        self.handle_item_export(name, amount);
    }
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
            threat: 0.0,
            generate_enemy_base: false,
            save: None,
            being_played: false,
            info: SectorInfoState::new(),
        }
    }

    /// Settings key `<planet>-s-<id>-info`.
    pub fn info_key(&self, planet_name: &str) -> String {
        format!("{planet_name}-s-{}-info", self.id)
    }

    /// `Sector.loadInfo`.
    pub fn load_info(&mut self, settings: &SettingsStore, planet_name: &str) {
        self.info.info = settings.get_json_or(&self.info_key(planet_name), SectorInfo::default);
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

    /// `Sector.isShielded`.
    pub fn is_shielded(&self) -> bool {
        self.shield_target.is_some() && !self.is_captured(None)
    }

    /// `Sector.displayThreat` (threat bucket; color is the caller's).
    pub fn threat_bucket(&self) -> &'static str {
        const THREATS: [&str; 5] = ["low", "medium", "high", "extreme", "eradication"];
        if self.preset_difficulty == 10.0 {
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

/// Packs a `(x, y)` tile position the way `Structs.pack`/`Point2.pack` does
/// (`y << 16 | x & 0xffff`).
pub const fn pack_position(x: i32, y: i32) -> i32 {
    (x & 0xffff) | (y << 16)
}

/// Unpacks a packed position.
pub const fn unpack_position(packed: i32) -> (i32, i32) {
    (packed & 0xffff, packed >> 16)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;

    #[test]
    fn info_windows_update_and_cap() {
        let mut state = SectorInfoState::new();
        // Seed export/import counters then refresh.
        state.handle_item_export("silicon", 30);
        state.handle_item_import("lead", 10);
        assert_eq!(state.get_export("silicon"), 30.0);

        // `handleItemExport` is a counter; updateStats moves it through the window.
        state
            .info
            .raw_production
            .entry("silicon".to_owned())
            .or_default()
            .mean = 5.0;
        state.windows.refresh_clock = REFRESH_PERIOD;
        state.update(0.0, &|name| match name {
            "silicon" => Some(0),
            "lead" => Some(1),
            _ => None,
        });
        // Export was capped by raw production + core draw (draw is 0).
        assert!(state.get_export("silicon") <= 5.0 + 1e-3);
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
        };
        state.prepare(&core, false);
        assert_eq!(state.info.items.get("copper"), Some(&250));
        assert_eq!(state.info.storage_capacity, 4000);
        assert_eq!(state.info.spawn_position, pack_position(32, 48));
        assert_eq!(unpack_position(state.info.spawn_position), (32, 48));
        assert_eq!(state.info.light_coverage, 3.5);

        let target = WriteTarget {
            wave_team_has_core: false,
            planet_allow_waves: true,
            preset_capture_wave: 0,
        };
        state.write(&target, 7, -1, 7200.0, true, false);
        assert_eq!(state.info.wave, 7);
        assert_eq!(
            state.info.win_wave, 30,
            "infinite waves get a default win wave"
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
}
