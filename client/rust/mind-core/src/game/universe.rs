// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Universe` + campaign container (plan 12 M3).
//!
//! Ported from `core/src/mindustry/game/Universe.java`. `Universe` owns global
//! seconds/turn counters, launch resources and loadouts; [`Campaign`] owns the
//! runtime planets/sectors and runs the deterministic production/import/invasion
//! passes of `Universe.runTurn`.
//!
//! Upstream drives time from `Time.delta` (frame-based). This port advances
//! counters in fixed 60 Hz ticks (`TURN_DURATION_TICKS`) so the sim stays
//! deterministic (HLP §2.4, plan 12 deviation 11); `universe::update_global`
//! replaces plan 05's `UniverseGlobal` stub system.

use indexmap::IndexMap;

use super::planet::{Planet, SectorNeighborhood};
use super::rules::{BASE_INVASION_CHANCE, INVASION_GRACE_PERIOD, TURN_DURATION_TICKS};
use super::sector::Sector;
use super::stats::CampaignStats;
use crate::content::{ContentRegistry, PlanetId};
use crate::io::settings::SettingsStore;
use crate::random::JavaRandom;

/// Settings key for elapsed campaign seconds (`utimei`).
pub const KEY_SECONDS: &str = "utimei";
/// Settings key for the campaign turn counter (`turn`).
pub const KEY_TURN: &str = "turn";
/// Settings key for the legacy launch-resource sequence (`launch-resources-seq`).
pub const KEY_LAUNCH_RESOURCES: &str = "launch-resources-seq";

/// Global campaign clock and launch state (`Universe`).
#[derive(Debug, Clone, PartialEq)]
pub struct Universe {
    /// Accumulated campaign seconds (`utimei`).
    pub seconds: i32,
    /// Networked seconds (clients use `net.client() ? netSeconds : seconds`).
    pub net_seconds: i32,
    /// Sub-second accumulator.
    pub second_counter: f32,
    /// Completed turns.
    pub turn: i32,
    /// Ticks accumulated toward the next auto turn.
    pub turn_counter: f32,
    /// Last launch resources by item index (`launch-resources-seq`).
    pub last_launch_resources: Vec<i32>,
    /// Last selected loadout file name.
    pub last_loadout: Option<String>,
}

impl Default for Universe {
    fn default() -> Self {
        Self::new()
    }
}

impl Universe {
    /// Empty universe (`Universe()` ctor before `load`).
    pub fn new() -> Self {
        Self {
            seconds: 0,
            net_seconds: 0,
            second_counter: 0.0,
            turn: 0,
            turn_counter: 0.0,
            last_launch_resources: Vec::new(),
            last_loadout: None,
        }
    }

    /// `Universe.load` (`utimei`/`turn` settings keys).
    pub fn load(&mut self, settings: &SettingsStore) {
        self.seconds = settings.get_i32(KEY_SECONDS, 0);
        self.turn = settings.get_i32(KEY_TURN, 0);
    }

    /// `Universe.save`.
    pub fn save(&self, settings: &mut SettingsStore) {
        settings.put_i32(KEY_SECONDS, self.seconds);
        settings.put_i32(KEY_TURN, self.turn);
    }

    /// `Universe.seconds()` (networked on clients).
    pub fn seconds(&self, is_client: bool) -> i32 {
        if is_client {
            self.net_seconds
        } else {
            self.seconds
        }
    }

    /// `Universe.secondsf()`.
    pub fn secondsf(&self, is_client: bool) -> f32 {
        self.seconds(is_client) as f32 + self.second_counter
    }

    /// `Universe.secondsMod`.
    pub fn seconds_mod(&self, is_client: bool, modulus: f32, scale: f32) -> f32 {
        (self.seconds(is_client) as f32 / scale) % modulus
    }

    /// `Universe.setSeconds`.
    pub fn set_seconds(&mut self, settings: &mut SettingsStore, seconds: f32) {
        self.seconds = seconds as i32;
        self.second_counter = seconds - self.seconds as f32;
        self.save(settings);
    }

    /// `Universe.updateNetSeconds`.
    pub fn update_net_seconds(&mut self, value: i32) {
        self.net_seconds = value;
    }

    /// `Universe.getLaunchResources` (reads the settings sequence).
    pub fn get_launch_resources(&mut self, settings: &SettingsStore) -> &[i32] {
        if let Ok(Some(sequence)) = settings.get_json::<Vec<i32>>(KEY_LAUNCH_RESOURCES) {
            self.last_launch_resources = sequence;
        }
        &self.last_launch_resources
    }

    /// `Universe.updateLaunchResources`.
    pub fn update_launch_resources(
        &mut self,
        settings: &mut SettingsStore,
        stacks: Vec<i32>,
    ) -> Result<(), crate::io::IoError> {
        self.last_launch_resources = stacks;
        settings.put_json(KEY_LAUNCH_RESOURCES, &self.last_launch_resources)
    }

    /// `Universe.clearLoadoutInfo`.
    pub fn clear_loadout_info(&mut self, settings: &mut SettingsStore) {
        self.last_loadout = None;
        self.last_launch_resources.clear();
        settings.remove(KEY_LAUNCH_RESOURCES);
        settings.remove("lastloadout-core-shard");
        settings.remove("lastloadout-core-nucleus");
        settings.remove("lastloadout-core-foundation");
    }

    /// `Universe.updateLoadout` (records the file name for a core type).
    pub fn update_loadout(
        &mut self,
        settings: &mut SettingsStore,
        core_name: &str,
        file_name: Option<&str>,
    ) {
        settings.put_string(&format!("lastloadout-{core_name}"), file_name.unwrap_or(""));
        self.last_loadout = file_name.map(str::to_owned);
    }

    /// `Universe.getLastLoadout` (name only; schematic resolution is plan 12 M5).
    pub fn get_last_loadout(&self) -> Option<&str> {
        self.last_loadout.as_deref()
    }

    /// `Universe.getLoadout` file name for a core type.
    pub fn get_loadout(&self, settings: &SettingsStore, core_name: &str) -> String {
        settings.get_string(&format!("lastloadout-{core_name}"), "")
    }

    /// Advances the fixed-step clock by `delta_ticks`; returns whether an auto
    /// turn is due (`turnCounter >= turnDuration`).
    ///
    /// Upstream also saves every 10 s; the save is the caller's (M4 policy).
    pub fn advance(&mut self, delta_ticks: u64) -> bool {
        let delta_seconds = delta_ticks as f32 / 60.0;
        self.second_counter += delta_seconds;
        self.turn_counter += delta_ticks as f32;

        if self.second_counter >= 1.0 {
            self.seconds += self.second_counter as i32;
            self.second_counter %= 1.0;
        }

        if self.turn_counter >= TURN_DURATION_TICKS as f32 {
            self.turn_counter = 0.0;
            true
        } else {
            false
        }
    }

    /// `Universe.update` lighting half: writes `ambientLight.a`/`lighting`.
    pub fn apply_lighting(&self, planet: &Planet, light: f32, rules: &mut super::rules::Rules) {
        if !planet.update_lighting {
            return;
        }
        let alpha = map_clamped(
            light,
            planet.light_src_from,
            planet.light_src_to,
            planet.light_dst_from,
            planet.light_dst_to,
        );
        rules.ambient_light.0.a = 1.0 - alpha;
        rules.lighting = (alpha - 1.0).abs() > f32::EPSILON;
    }
}

/// `Mathf.map(value, il, ih, ol, oh)` clamped to `[0, 1]`.
fn map_clamped(value: f32, il: f32, ih: f32, ol: f32, oh: f32) -> f32 {
    if (ih - il).abs() < f32::EPSILON {
        return 0.0;
    }
    let mapped = ol + (value - il) * (oh - ol) / (ih - il);
    mapped.clamp(0.0, 1.0)
}

/// Which sector/scenario the turn is running in.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TurnContext {
    /// Planet currently being played, if any.
    pub current_planet: Option<PlanetId>,
    /// Sector currently being played, if any.
    pub current_sector: Option<(PlanetId, u16)>,
    /// Live `rules.waves`/`rules.attackMode` when playing.
    pub playing_rules: Option<(bool, bool)>,
    /// Live `state.wave` when playing.
    pub current_wave: i32,
}

/// Campaign events emitted by a turn (`TurnEvent`/`SectorInvasionEvent`).
#[derive(Debug, Clone, PartialEq)]
pub enum CampaignEvent {
    /// A turn completed (`TurnEvent`).
    Turn,
    /// A sector was queued for an invasion (`SectorInvasionEvent`).
    SectorInvasion {
        /// Invaded planet.
        planet: PlanetId,
        /// Invaded sector.
        sector: u16,
        /// New win wave.
        win_wave: i32,
    },
}

/// Deterministic report from [`Campaign::run_turn`].
#[derive(Debug, Clone, PartialEq)]
pub struct TurnReport {
    /// New turn number.
    pub turn: i32,
    /// Units of seconds passed during this turn (`turnDuration / 60`).
    pub seconds_passed: f32,
    /// Events in emission order.
    pub events: Vec<CampaignEvent>,
}

/// The runtime campaign: planets, sectors and the global clock.
#[derive(Debug, Clone, PartialEq)]
pub struct Campaign {
    /// Global universe clock/launch state.
    pub universe: Universe,
    /// Planets keyed by dense id, in content order.
    pub planets: IndexMap<u16, Planet>,
    /// Planet ids in content order (root-first).
    pub order: Vec<u16>,
}

impl Campaign {
    /// Builds the runtime campaign from the content registry.
    pub fn from_registry(
        registry: &ContentRegistry,
        neighborhood: &dyn SectorNeighborhood,
    ) -> Self {
        let mut planets: IndexMap<u16, Planet> = IndexMap::new();
        let mut order = Vec::new();
        let mut total_radii: IndexMap<u16, f32> = IndexMap::new();

        for def in registry.planets() {
            let parent_total = def
                .parent
                .and_then(|id| total_radii.get(&id.raw()).copied())
                .unwrap_or(0.0);
            let mut planet = Planet::from_def(registry, def, parent_total);
            // Build adjacency from the icosphere grid when the planet has one;
            // fall back to the plan-06 `SectorNeighborhood` seam otherwise.
            planet.neighbors = if def.sector_tiles > 0 {
                let grid = crate::render::g3d::grid::PlanetGrid::create(def.sector_tiles as usize);
                (0..planet.sector_count())
                    .map(|id| {
                        grid.tiles
                            .get(id as usize)
                            .map(|tile| tile.tiles.iter().map(|n| *n as u16).collect())
                            .unwrap_or_default()
                    })
                    .collect()
            } else {
                (0..planet.sector_count())
                    .map(|id| neighborhood.near(def.id, id as u16))
                    .collect()
            };
            total_radii.insert(def.id.raw(), planet.total_radius);
            order.push(def.id.raw());
            if let Some(parent) = def.parent
                && let Some(parent_planet) = planets.get_mut(&parent.raw())
            {
                parent_planet.children.push(def.id);
            }
            planets.insert(def.id.raw(), planet);
        }

        Self {
            universe: Universe::new(),
            planets,
            order,
        }
    }

    /// Planet by id.
    pub fn planet(&self, id: PlanetId) -> Option<&Planet> {
        self.planets.get(&id.raw())
    }

    /// Mutable planet by id.
    pub fn planet_mut(&mut self, id: PlanetId) -> Option<&mut Planet> {
        self.planets.get_mut(&id.raw())
    }

    /// Planet id by content name.
    pub fn planet_id_by_name(&self, name: &str) -> Option<PlanetId> {
        self.planets
            .values()
            .find(|planet| planet.name == name)
            .map(|planet| planet.id)
    }

    /// Sector by `(planet, id)`.
    pub fn sector(&self, planet: PlanetId, id: u16) -> Option<&Sector> {
        self.planet(planet).and_then(|planet| planet.sector(id))
    }

    /// Mutable sector by `(planet, id)`.
    pub fn sector_mut(&mut self, planet: PlanetId, id: u16) -> Option<&mut Sector> {
        self.planet_mut(planet)
            .and_then(|planet| planet.sector_mut(id))
    }

    /// `Planet.stats()` with `statParent` delegation.
    pub fn stats_for(&self, planet: PlanetId) -> Option<&CampaignStats> {
        let record = self.planet(planet)?;
        match record.stat_parent {
            Some(parent) if parent != planet => self.planet(parent).map(Planet::own_stats),
            _ => Some(record.own_stats()),
        }
    }

    /// Mutable stats with `statParent` delegation.
    pub fn stats_for_mut(&mut self, planet: PlanetId) -> Option<&mut CampaignStats> {
        let parent = self.planet(planet).and_then(|record| record.stat_parent);
        match parent {
            Some(parent) if parent != planet => self.planet_mut(parent).map(|p| &mut p.stats),
            _ => self.planet_mut(planet).map(|p| &mut p.stats),
        }
    }

    /// `Universe.updateGlobal`: positions every parentless planet recursively.
    pub fn update_global(&mut self, seconds: f32) {
        let roots: Vec<u16> = self
            .order
            .iter()
            .copied()
            .filter(|id| {
                self.planets
                    .get(id)
                    .is_some_and(|planet| planet.parent.is_none())
            })
            .collect();
        for root in roots {
            let position = self
                .planets
                .get(&root)
                .map(|planet| {
                    planet.add_parent_offset(crate::io::json::rules::Vec3::default(), seconds)
                })
                .unwrap_or_default();
            self.set_position_recursive(root, position, seconds);
        }
    }

    fn set_position_recursive(
        &mut self,
        id: u16,
        position: crate::io::json::rules::Vec3,
        seconds: f32,
    ) {
        let children = if let Some(planet) = self.planets.get_mut(&id) {
            planet.position = position;
            planet.children.clone()
        } else {
            return;
        };
        for child in children {
            let child_position = self
                .planets
                .get(&child.raw())
                .map(|planet| planet.add_parent_offset(position, seconds))
                .unwrap_or(position);
            self.set_position_recursive(child.raw(), child_position, seconds);
        }
    }

    /// `Universe.update` seconds/turn half; returns whether a turn is due.
    pub fn advance_time(&mut self, delta_ticks: u64) -> bool {
        self.universe.advance(delta_ticks)
    }

    /// Refreshes a sector's import rate cache from every base exporting to it.
    pub fn refresh_sector_import_rates(
        &mut self,
        registry: &ContentRegistry,
        planet: PlanetId,
        sector_id: u16,
    ) {
        let destination = (planet, sector_id);
        let sources: Vec<(String, f32)> = self
            .sectors_of(planet)
            .flat_map(|(_, sector)| {
                if sector.has_base()
                    && sector.info.info.destination.is_some()
                    && sector.info.any_exports()
                {
                    let is_dest = sector
                        .info
                        .info
                        .destination
                        .as_ref()
                        .and_then(|key| key.resolve(registry))
                        == Some(destination);
                    if is_dest {
                        return sector
                            .info
                            .info
                            .export
                            .iter()
                            .map(|(name, stat)| (name.clone(), stat.mean))
                            .collect::<Vec<_>>();
                    }
                }
                Vec::new()
            })
            .collect();
        let item_count = registry.items().len();
        let index_of = |name: &str| registry.item_id(name).map(|id| id.index());
        if let Some(sector) = self.sector_mut(planet, sector_id) {
            sector.info.refresh_import_rates(
                sources.iter().map(|(n, m)| (n.as_str(), *m)),
                item_count,
                index_of,
            );
        }
    }

    fn sectors_of(&self, planet: PlanetId) -> impl Iterator<Item = (u16, &Sector)> {
        self.planet(planet)
            .into_iter()
            .flat_map(|planet| planet.sectors.iter().map(move |sector| (sector.id, sector)))
    }

    /// `Universe.runTurn`: production/import/invasion passes.
    pub fn run_turn(
        &mut self,
        registry: &ContentRegistry,
        settings: &mut SettingsStore,
        ctx: &TurnContext,
        rand: &mut JavaRandom,
    ) -> TurnReport {
        self.universe.turn += 1;
        let turn = self.universe.turn;
        let seconds_passed = (TURN_DURATION_TICKS / 60) as f32;
        let mut events = Vec::new();

        let planet_ids: Vec<u16> = self.order.clone();
        for planet_raw in planet_ids {
            let planet_id = PlanetId::new(planet_raw);
            if let Some(current) = ctx.current_planet
                && current != planet_id
            {
                // Vanilla `updateGroup` is empty: other planets are skipped.
                continue;
            }
            let legacy_pads = self
                .planet(planet_id)
                .is_some_and(|p| p.campaign_rules.legacy_launch_pads);

            if legacy_pads {
                self.clear_imports_pass(planet_id);
                self.export_import_pass(planet_id, registry, seconds_passed);
            }

            self.production_pass(planet_id, registry, settings, ctx, seconds_passed);
        }

        // Random invasions are queued after production (upstream pass 3).
        self.invasion_pass(registry, ctx, seconds_passed, rand, &mut events);

        events.push(CampaignEvent::Turn);
        self.universe.save(settings);
        TurnReport {
            turn,
            seconds_passed,
            events,
        }
    }

    fn clear_imports_pass(&mut self, planet: PlanetId) {
        if let Some(planet) = self.planet_mut(planet) {
            for sector in &mut planet.sectors {
                if sector.has_base() && !sector.is_being_played() {
                    sector.info.last_imported.clear();
                }
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn export_import_pass(
        &mut self,
        planet: PlanetId,
        registry: &ContentRegistry,
        seconds_passed: f32,
    ) {
        // Collect transfers first (immutable), then apply (mutable).
        let mut transfers: Vec<((PlanetId, u16), Vec<(String, i32)>)> = Vec::new();
        if let Some(planet_record) = self.planet(planet) {
            for sector in &planet_record.sectors {
                if !sector.has_base() || sector.is_being_played() || sector.is_attacked(None) {
                    continue;
                }
                let Some(dest_key) = sector.info.info.destination.as_ref() else {
                    continue;
                };
                let Some(dest) = dest_key.resolve(registry) else {
                    continue;
                };
                if dest.0 != planet {
                    continue;
                }
                let Some(dest_sector) = self.sector(dest.0, dest.1) else {
                    continue;
                };
                if !dest_sector.has_base() {
                    continue;
                }
                let items: Vec<(String, i32)> = sector
                    .info
                    .info
                    .export
                    .iter()
                    .map(|(name, stat)| (name.clone(), (stat.mean * seconds_passed) as i32))
                    .collect();
                transfers.push(((dest.0, dest.1), items));
            }
        }
        for (dest, items) in transfers {
            if let Some(sector) = self.sector_mut(dest.0, dest.1) {
                for (name, amount) in &items {
                    *sector.info.last_imported.entry(name.clone()).or_insert(0) += amount;
                    let capacity = sector.info.info.storage_capacity;
                    let slot = sector.info.info.items.entry(name.clone()).or_insert(0);
                    *slot = (*slot + amount).min(capacity).max(0);
                }
            }
        }
    }

    fn production_pass(
        &mut self,
        planet: PlanetId,
        registry: &ContentRegistry,
        settings: &mut SettingsStore,
        ctx: &TurnContext,
        seconds_passed: f32,
    ) {
        let planet_name = self
            .planet(planet)
            .map(|p| p.name.clone())
            .unwrap_or_default();

        let ids: Vec<u16> = self
            .planet(planet)
            .map(|p| p.sectors.iter().map(|s| s.id).collect())
            .unwrap_or_default();

        for id in ids {
            let has_base = self.sector(planet, id).is_some_and(|s| s.has_base());
            if !has_base {
                continue;
            }
            let is_attacked = self
                .sector(planet, id)
                .is_some_and(|s| s.is_attacked(ctx.playing_rules));
            let being_played = self.sector(planet, id).is_some_and(Sector::is_being_played);

            if is_attacked {
                if let Some(sector) = self.sector_mut(planet, id) {
                    sector.info.info.minutes_captured = 0.0;
                }
            } else if let Some(sector) = self.sector_mut(planet, id) {
                sector.info.info.minutes_captured += TURN_DURATION_TICKS as f32 / 60.0 / 60.0;
            }

            if is_attacked || being_played {
                continue;
            }

            // Refresh import rates for the sector.
            self.refresh_sector_import_rates(registry, planet, id);

            // Production capped by capacity.
            let productions: Vec<(String, f32)> = self
                .sector(planet, id)
                .map(|s| {
                    s.info
                        .info
                        .production
                        .iter()
                        .map(|(name, stat)| (name.clone(), stat.mean))
                        .collect()
                })
                .unwrap_or_default();
            if let Some(sector) = self.sector_mut(planet, id) {
                let capacity = sector.info.info.storage_capacity;
                for (name, mean) in productions {
                    let amount = (mean * seconds_passed) as i32;
                    let slot = sector.info.info.items.entry(name).or_insert(0);
                    *slot = (*slot + amount).min(capacity).max(0);
                }
                for value in sector.info.info.items.values_mut() {
                    *value = (*value).max(0);
                }
                let _ = sector.save_info(settings, &planet_name);
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn invasion_pass(
        &mut self,
        registry: &ContentRegistry,
        _ctx: &TurnContext,
        seconds_passed: f32,
        rand: &mut JavaRandom,
        events: &mut Vec<CampaignEvent>,
    ) {
        let _ = registry;
        let _ = seconds_passed;
        // Gather candidates first to avoid borrowing self mutably while reading.
        let mut candidates: Vec<(PlanetId, u16, i32, bool, bool)> = Vec::new();
        for planet in self.planets.values() {
            if !planet.campaign_rules.sector_invasion {
                continue;
            }
            for sector in &planet.sectors {
                if !sector.has_base()
                    || sector.is_attacked(None)
                    || sector.info.info.minutes_captured <= INVASION_GRACE_PERIOD
                    || !sector.info.info.has_spawns
                {
                    continue;
                }
                let count = planet
                    .neighbors
                    .get(sector.id as usize)
                    .into_iter()
                    .flatten()
                    .filter(|&&n| {
                        planet.sectors.get(n as usize).is_some_and(|s| {
                            s.has_enemy_base()
                                && !s.has_base()
                                && (s.preset.is_none() || !s.preset_require_unlock)
                        })
                    })
                    .count();
                if count == 0 {
                    continue;
                }
                let chance = BASE_INVASION_CHANCE * (0.8 + (count as f64 - 1.0) * 0.3);
                if rand.next_double() < chance {
                    let base = sector.info.info.win_wave.max(sector.info.info.wave);
                    let wave_max = base + (2 + rand.next_int_bound(3)) * 5;
                    candidates.push((planet.id, sector.id, wave_max, false, false));
                }
            }
        }
        for (planet, sector_id, wave_max, _, _) in candidates {
            if let Some(sector) = self.sector_mut(planet, sector_id) {
                sector.info.info.win_wave = wave_max;
                sector.info.info.waves = true;
                sector.info.info.attack = false;
            }
            events.push(CampaignEvent::SectorInvasion {
                planet,
                sector: sector_id,
                win_wave: wave_max,
            });
        }
    }

    /// Persists every planet's campaign rules and stats.
    pub fn save_all(&self, settings: &mut SettingsStore) -> Result<(), crate::io::IoError> {
        for planet in self.planets.values() {
            planet.save_rules(settings)?;
            planet.save_stats(settings)?;
        }
        self.universe.save(settings);
        Ok(())
    }

    /// Loads every planet's campaign rules and stats, then universe counters.
    pub fn load_all(&mut self, settings: &SettingsStore) {
        for planet in self.planets.values_mut() {
            planet.load_rules(settings);
            planet.load_stats(settings);
        }
        self.universe.load(settings);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::io::json::rules::ExportStat;

    fn make_campaign() -> (ContentRegistry, Campaign) {
        let registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).unwrap();
        let campaign = Campaign::from_registry(&registry, &crate::game::planet::EmptyNeighborhood);
        (registry, campaign)
    }

    fn serpulo() -> (ContentRegistry, Campaign, PlanetId) {
        let (registry, campaign) = make_campaign();
        let id = campaign.planet_id_by_name("serpulo").unwrap();
        (registry, campaign, id)
    }

    #[test]
    fn universe_advance_and_settings_roundtrip() {
        let mut settings = SettingsStore::new();
        let mut universe = Universe::new();
        // 60 ticks = 1 second.
        assert!(!universe.advance(60));
        assert_eq!(universe.seconds, 1);
        // 7200 ticks per turn (including the first 60).
        assert!(
            universe.advance(TURN_DURATION_TICKS - 60),
            "turn becomes due exactly at the duration"
        );
        universe.turn += 1;
        universe.save(&mut settings);

        let mut reloaded = Universe::new();
        reloaded.load(&settings);
        assert_eq!(reloaded.seconds, 120);
        assert_eq!(reloaded.turn, 1);
    }

    #[test]
    fn update_global_positions_children() {
        let (registry, mut campaign) = make_campaign();
        let mut settings = SettingsStore::new();
        campaign.universe.set_seconds(&mut settings, 500.0);
        campaign.update_global(500.0);
        let sun = campaign.planet_id_by_name("sun").unwrap();
        let serpulo = campaign.planet_id_by_name("serpulo").unwrap();
        assert_eq!(campaign.planet(sun).unwrap().position, Default::default());
        let child_position = campaign.planet(serpulo).unwrap().position;
        assert!(
            child_position.x != 0.0 || child_position.z != 0.0,
            "child planet orbits its parent"
        );
        // erekir's child gier receives an accumulated offset.
        let gier = campaign.planet_id_by_name("gier").unwrap();
        assert!(campaign.planet(gier).unwrap().parent.is_some());
        let _ = registry;
    }

    #[test]
    fn run_turn_produces_caps_and_exports() {
        let (registry, mut campaign, planet) = serpulo();
        let mut settings = SettingsStore::new();

        // Two owned sectors; sector A produces silicon, B is its destination.
        let source_id = 15u16;
        let dest_id = 16u16;
        for id in [source_id, dest_id] {
            let sector = campaign.sector_mut(planet, id).unwrap();
            sector.save = Some(format!("sector-serpulo-{id}"));
            sector.info.info.has_core = true;
            sector.info.info.storage_capacity = 1000;
            // A captured/owned sector has waves and attack mode off.
            sector.info.info.waves = false;
            sector.info.info.attack = false;
        }
        let source_key = crate::io::json::content_serde::SectorKey::format("serpulo", dest_id);
        {
            let sector = campaign.sector_mut(planet, source_id).unwrap();
            sector
                .info
                .info
                .production
                .insert("silicon".to_owned(), ExportStat { mean: 2.5 });
            sector
                .info
                .info
                .export
                .insert("silicon".to_owned(), ExportStat { mean: 2.5 });
            sector.info.info.destination = Some(source_key);
        }
        // Legacy launch pads drive the import/export pass.
        campaign
            .planet_mut(planet)
            .unwrap()
            .campaign_rules
            .legacy_launch_pads = true;

        let report = campaign.run_turn(
            &registry,
            &mut settings,
            &TurnContext::default(),
            &mut JavaRandom::new(7),
        );
        assert_eq!(report.turn, 1);
        // 2.5/s * 120s = 300 items produced and exported.
        let dest = campaign.sector(planet, dest_id).unwrap();
        assert_eq!(dest.info.info.items.get("silicon"), Some(&300));
        let source = campaign.sector(planet, source_id).unwrap();
        assert_eq!(source.info.info.items.get("silicon"), Some(&300));
        // TurnEvent emitted.
        assert_eq!(report.events.last(), Some(&CampaignEvent::Turn));
    }

    #[test]
    fn campaign_rules_and_stats_persist() {
        let (_registry, mut campaign, planet) = serpulo();
        let mut settings = SettingsStore::new();
        {
            let p = campaign.planet_mut(planet).unwrap();
            p.campaign_rules.difficulty = super::super::campaign_rules::Difficulty::Hard;
            p.stats.sectors_captured = 3;
        }
        campaign.save_all(&mut settings).unwrap();

        let (_registry2, mut reloaded) = make_campaign();
        reloaded.load_all(&settings);
        let p = reloaded.planet(planet).unwrap();
        assert_eq!(
            p.campaign_rules.difficulty,
            super::super::campaign_rules::Difficulty::Hard
        );
        assert_eq!(p.stats.sectors_captured, 3);
    }
}
