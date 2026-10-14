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
}

/// `LaunchLoadoutDialog.update` (`LaunchLoadoutDialog.java:58-90`), item maps
/// keyed by content name.
///
/// Returns the launch resources to stage (`resources.min(capacity)`, the
/// `max` recomputation from the launcher's stored items minus the schematic
/// cost, or the destination preset loadout when the destination forbids a
/// launch loadout) and whether the launcher can pay `requirements +
/// resources` (`valid`).
#[allow(clippy::too_many_arguments)]
pub fn launch_plan(
    stored: &IndexMap<String, i32>,
    requirements: &IndexMap<String, i32>,
    resources: &IndexMap<String, i32>,
    capacity: i32,
    max: bool,
    allow_launch_loadout: bool,
    preset_loadout: &IndexMap<String, i32>,
    all_items: &[String],
) -> (IndexMap<String, i32>, bool) {
    let cap = capacity.max(0);
    // `resources.min(cap)`: the staged sequence is clamped to the capacity.
    let mut out: IndexMap<String, i32> = resources
        .iter()
        .map(|(item, amount)| (item.clone(), (*amount).clamp(0, cap)))
        .collect();
    if !allow_launch_loadout {
        // `resources.clear()` + the destination preset's map `rules.loadout`
        // (filtered to items on the launcher's planet by the caller).
        out.clear();
        for (item, amount) in preset_loadout {
            if *amount != 0 {
                out.insert(item.clone(), *amount);
            }
        }
    } else if max {
        // `resources.set(item, clamp(sitems.get(item) - schems.get(item), 0, capacity))`
        for item in all_items {
            let stored_amount = stored.get(item).copied().unwrap_or(0);
            let requirement = requirements.get(item).copied().unwrap_or(0);
            let value = (stored_amount - requirement).clamp(0, cap);
            out.insert(item.clone(), value);
        }
    }
    // `total = requirements + resources`; `valid = sitems.has(total)`.
    let mut total = requirements.clone();
    for (item, amount) in &out {
        *total.entry(item.clone()).or_insert(0) += *amount;
    }
    let valid = total
        .iter()
        .all(|(item, amount)| stored.get(item).copied().unwrap_or(0) >= *amount && *amount >= 0);
    (out, valid)
}

impl Universe {
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
    ///
    /// `played` is true when the invaded sector is the one being played: Java
    /// then writes the live rules instead of the stored record, exactly once
    /// per invasion (`Universe.java:231-258`).
    SectorInvasion {
        /// Invaded planet.
        planet: PlanetId,
        /// Invaded sector.
        sector: u16,
        /// New win wave.
        win_wave: i32,
        /// Invaded sector is the live one.
        played: bool,
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
                            .get(id)
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

    /// `Planet.clearStats` with `statParent` delegation (`SettingsMenuDialog`
    /// clear/clear-planet campaign saves).
    pub fn clear_stats(&mut self, planet: PlanetId) {
        let parent = self.planet(planet).and_then(|record| record.stat_parent);
        match parent {
            Some(parent) if parent != planet => {
                if let Some(parent) = self.planet_mut(parent) {
                    parent.clear_stats();
                }
            }
            _ => {
                if let Some(record) = self.planet_mut(planet) {
                    record.clear_stats();
                }
            }
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
        self.invasion_pass(registry, ctx, seconds_passed, rand, settings, &mut events);

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
        ctx: &TurnContext,
        seconds_passed: f32,
        rand: &mut JavaRandom,
        settings: &mut SettingsStore,
        events: &mut Vec<CampaignEvent>,
    ) {
        let _ = registry;
        let _ = seconds_passed;
        // Gather candidates first to avoid borrowing self mutably while reading.
        // `Regions.current.updateGroup` is empty in vanilla, so planets outside
        // the played one are skipped (`Universe.java:160-163`).
        let mut candidates: Vec<(PlanetId, u16, i32, bool)> = Vec::new();
        for planet in self.planets.values() {
            if !planet.campaign_rules.sector_invasion {
                continue;
            }
            if let Some(current) = ctx.current_planet
                && current != planet.id
            {
                continue;
            }
            for sector in &planet.sectors {
                let played = ctx.current_sector == Some((planet.id, sector.id));
                if !sector.has_base()
                    || sector.is_attacked(if played { ctx.playing_rules } else { None })
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
                    // `Math.max(sector.info.winWave, isBeingPlayed() ?
                    // state.wave : sector.info.wave) + Mathf.random(2, 4) * 5`.
                    let base = sector.info.info.win_wave.max(if played {
                        ctx.current_wave
                    } else {
                        sector.info.info.wave
                    });
                    let wave_max = base + (2 + rand.next_int_bound(3)) * 5;
                    candidates.push((planet.id, sector.id, wave_max, played));
                }
            }
        }
        let mut to_save: Vec<(PlanetId, u16)> = Vec::new();
        for (planet, sector_id, wave_max, played) in candidates {
            if !played {
                // The off-play record takes the invasion; the played sector's
                // live rules are written by the host pump (`Universe.java:550-555`).
                if let Some(sector) = self.sector_mut(planet, sector_id) {
                    sector.info.info.win_wave = wave_max;
                    sector.info.info.waves = true;
                    sector.info.info.attack = false;
                }
                to_save.push((planet, sector_id));
            }
            events.push(CampaignEvent::SectorInvasion {
                planet,
                sector: sector_id,
                win_wave: wave_max,
                played,
            });
        }
        // `sector.saveInfo()` for every invaded record (`Universe.java:554`).
        for (planet_id, sector_id) in to_save {
            let Some(planet_name) = self.planet(planet_id).map(|planet| planet.name.clone()) else {
                continue;
            };
            if let Some(sector) = self.sector(planet_id, sector_id) {
                let _ = sector.save_info(settings, &planet_name);
            }
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
            // `Planet.load` -> each `sector.loadInfo()`: the stored per-sector
            // record (captures, attempts, wave progress, item pools, routes)
            // must survive a client restart (PAR-0081).
            let planet_name = planet.name.clone();
            for sector in &mut planet.sectors {
                sector.load_info(settings, &planet_name);
            }
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
    fn update_loadout_persists_key() {
        let mut settings = SettingsStore::new();
        let mut universe = Universe::new();
        // `Universe.updateLoadout(block, schem)`: `lastloadout-<core>` records
        // the schematic's file name (`Universe.java:124-127`).
        universe.update_loadout(&mut settings, "core-shard", Some("big"));
        assert_eq!(settings.get_string("lastloadout-core-shard", ""), "big");
        assert_eq!(universe.get_loadout(&settings, "core-shard"), "big");
        // A file-less schematic records the empty string (Java `schem.file == null`).
        universe.update_loadout(&mut settings, "core-shard", None);
        assert_eq!(settings.get_string("lastloadout-core-shard", ""), "");
        // `clearLoadoutInfo` drops the key and the launch-resource sequence.
        universe.update_loadout(&mut settings, "core-shard", Some("big"));
        universe
            .update_launch_resources(&mut settings, vec![1, 2, 3])
            .unwrap();
        universe.clear_loadout_info(&mut settings);
        assert_eq!(settings.get_string("lastloadout-core-shard", ""), "");
        assert!(
            settings
                .get_json::<Vec<i32>>(KEY_LAUNCH_RESOURCES)
                .unwrap()
                .is_none(),
            "launch-resources-seq removed"
        );
    }

    #[test]
    fn launch_plan_max_fills_to_capacity() {
        // `LaunchLoadoutDialog.update` with Max on: copper 3000 stored, shard
        // cost 1000/800, capacity 2000 -> copper 2000, lead 1200, valid.
        let stored = IndexMap::from([("copper".to_owned(), 3000), ("lead".to_owned(), 2000)]);
        let requirements = IndexMap::from([("copper".to_owned(), 1000), ("lead".to_owned(), 800)]);
        let resources = IndexMap::new();
        let all_items = vec!["copper".to_owned(), "lead".to_owned()];
        let (out, valid) = launch_plan(
            &stored,
            &requirements,
            &resources,
            2000,
            true,
            true,
            &IndexMap::new(),
            &all_items,
        );
        assert_eq!(out.get("copper").copied(), Some(2000));
        assert_eq!(out.get("lead").copied(), Some(1200));
        assert!(valid, "launcher holds requirements + resources");
    }

    #[test]
    fn launch_plan_invalid_when_short() {
        let requirements = IndexMap::from([("copper".to_owned(), 1000)]);
        let (out, valid) = launch_plan(
            &IndexMap::new(),
            &requirements,
            &IndexMap::new(),
            2000,
            true,
            true,
            &IndexMap::new(),
            &["copper".to_owned()],
        );
        assert_eq!(out.get("copper").copied(), Some(0));
        assert!(!valid, "empty launcher cannot pay the schematic cost");
    }

    #[test]
    fn launch_plan_disallowed_uses_preset_loadout() {
        // `!destination.allowLaunchLoadout()` clears the staged resources and
        // stages the destination preset's map `rules.loadout` instead.
        let stored = IndexMap::from([("copper".to_owned(), 500)]);
        let preset = IndexMap::from([("copper".to_owned(), 50), ("lead".to_owned(), 0)]);
        let (out, valid) = launch_plan(
            &stored,
            &IndexMap::from([("lead".to_owned(), 25)]),
            &IndexMap::from([("copper".to_owned(), 100)]),
            2000,
            true,
            false,
            &preset,
            &["copper".to_owned(), "lead".to_owned()],
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out.get("copper").copied(), Some(50));
        assert!(!valid, "lead 25 is not stored");
    }

    #[test]
    fn load_all_restores_sector_info() {
        let (registry, mut campaign, planet) = serpulo();
        let mut settings = SettingsStore::new();
        // Capture bookkeeping as the gameplay path leaves it on sector 15.
        let sector = campaign.sector_mut(planet, 15).unwrap();
        sector.info.info.was_captured = true;
        sector.info.info.attempts = 3;
        sector.info.info.items.insert("copper".to_owned(), 250);
        assert!(sector.save_info(&mut settings, "serpulo").is_ok());

        // A fresh boot (`MindCampaign::bootstrap` -> `Campaign::load_all`)
        // reloads the stored records (PAR-0081).
        let mut rebuilt =
            Campaign::from_registry(&registry, &crate::game::planet::EmptyNeighborhood);
        rebuilt.load_all(&settings);
        let record = &rebuilt.sector(planet, 15).unwrap().info.info;
        assert!(record.was_captured, "capture survives a restart");
        assert_eq!(record.attempts, 3);
        assert_eq!(record.items.get("copper"), Some(&250));
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
            sector.info.info.production.insert(
                "silicon".to_owned(),
                ExportStat {
                    mean: 2.5,
                    ..Default::default()
                },
            );
            sector.info.info.export.insert(
                "silicon".to_owned(),
                ExportStat {
                    mean: 2.5,
                    ..Default::default()
                },
            );
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

    #[test]
    fn clear_stats_follows_stat_parent() {
        let (registry, mut campaign, serpulo) = serpulo();
        let erekir = registry.planet_id("erekir").unwrap();
        campaign.planet_mut(serpulo).unwrap().stats.sectors_captured = 4;
        campaign.planet_mut(erekir).unwrap().stats.sectors_captured = 2;
        // A shared stats record clears the parent's aggregate for both planets.
        campaign.planet_mut(erekir).unwrap().stat_parent = Some(serpulo);

        campaign.clear_stats(erekir);
        assert_eq!(campaign.planet(serpulo).unwrap().stats.sectors_captured, 0);

        // A record with no stat parent clears its own stats.
        campaign.planet_mut(erekir).unwrap().stat_parent = None;
        campaign.planet_mut(erekir).unwrap().stats.sectors_captured = 2;
        campaign.planet_mut(serpulo).unwrap().stats.sectors_captured = 7;
        campaign.clear_stats(erekir);
        assert_eq!(campaign.planet(erekir).unwrap().stats.sectors_captured, 0);
        assert_eq!(campaign.planet(serpulo).unwrap().stats.sectors_captured, 7);
    }

    #[test]
    fn save_all_after_run_turn_keeps_turn() {
        let (registry, mut campaign, _planet) = serpulo();
        let mut settings = SettingsStore::new();
        // `stale_facade` is exactly what `MindCampaign.persist` used to write
        // while a sector was live (the facade's last-mirrored snapshot): its
        // `universe.turn` never advanced, so saving it reset the clock. The
        // facade now mirrors the runtime before saving (H04 T1).
        let stale_facade = campaign.clone();
        for _ in 0..2 {
            let turn = campaign.universe.turn;
            campaign.run_turn(
                &registry,
                &mut settings,
                &TurnContext::default(),
                &mut JavaRandom::new(turn as u64 + 1),
            );
        }
        campaign.save_all(&mut settings).unwrap();
        assert_eq!(settings.get_i32("turn", 0), 2);
        // The stale copy would write 0 back over the live turn counter.
        stale_facade.save_all(&mut settings).unwrap();
        assert_eq!(settings.get_i32("turn", 0), 0);
    }

    #[test]
    fn left_sector_produces_on_next_turn() {
        let (registry, mut campaign, planet) = serpulo();
        let mut settings = SettingsStore::new();
        let id = 15u16;
        {
            let sector = campaign.sector_mut(planet, id).unwrap();
            sector.save = Some(format!("sector-serpulo-{id}"));
            sector.info.info.has_core = true;
            sector.info.info.storage_capacity = 4000;
            sector.info.info.waves = false;
            sector.info.info.attack = false;
            sector.being_played = false;
            sector.info.info.production.insert(
                "copper".to_owned(),
                ExportStat {
                    mean: 1.0,
                    ..Default::default()
                },
            );
        }
        let report = campaign.run_turn(
            &registry,
            &mut settings,
            &TurnContext::default(),
            &mut JavaRandom::new(3),
        );
        assert_eq!(report.turn, 1);
        // `turnDuration / 60` = 120 s of mean 1.0/s production (`Universe.java:154,213`).
        assert_eq!(
            campaign
                .sector(planet, id)
                .unwrap()
                .info
                .info
                .items
                .get("copper"),
            Some(&120)
        );
        // A sector that is being played is skipped (`Sector.isBeingPlayed`).
        campaign.sector_mut(planet, id).unwrap().being_played = true;
        campaign.run_turn(
            &registry,
            &mut settings,
            &TurnContext::default(),
            &mut JavaRandom::new(4),
        );
        assert_eq!(
            campaign
                .sector(planet, id)
                .unwrap()
                .info
                .info
                .items
                .get("copper"),
            Some(&120),
            "no offline production for the played sector"
        );
    }

    /// A sector with a base, a live invasion clock and one enemy-base neighbour
    /// (the `Universe.runTurn` invasion fixture).
    fn invasion_setup(campaign: &mut Campaign, planet: PlanetId) -> (u16, u16) {
        campaign
            .planet_mut(planet)
            .unwrap()
            .campaign_rules
            .sector_invasion = true;
        let (sector_id, neighbor_id) = {
            let planet_record = campaign.planet(planet).unwrap();
            let sector_id = (0..planet_record.sectors.len() as u16)
                .find(|id| {
                    planet_record
                        .neighbors
                        .get(*id as usize)
                        .is_some_and(|neighbors| !neighbors.is_empty())
                })
                .expect("a sector with a neighbour");
            let neighbor_id = planet_record.neighbors[sector_id as usize][0];
            (sector_id, neighbor_id)
        };
        {
            let sector = campaign.sector_mut(planet, sector_id).unwrap();
            sector.save = Some(format!("sector-serpulo-{sector_id}"));
            sector.info.info.has_core = true;
            sector.info.info.waves = false;
            sector.info.info.attack = false;
            sector.info.info.minutes_captured = 30.0;
            sector.info.info.has_spawns = true;
            sector.info.info.win_wave = 10;
            sector.info.info.wave = 5;
            sector.being_played = false;
        }
        {
            let neighbor = campaign.sector_mut(planet, neighbor_id).unwrap();
            neighbor.save = None;
            neighbor.preset = None;
            neighbor.generate_enemy_base = true;
        }
        (sector_id, neighbor_id)
    }

    #[test]
    fn invasion_of_played_sector_targets_live_rules() {
        let (registry, mut campaign, planet) = serpulo();
        let mut settings = SettingsStore::new();
        let (sector_id, _neighbor_id) = invasion_setup(&mut campaign, planet);
        // Seed whose first draw triggers the base chance (0.01 * 0.8).
        let seed = (0..10_000u64)
            .find(|seed| JavaRandom::new(*seed).next_double() < 0.8 * BASE_INVASION_CHANCE)
            .expect("a seed in range");
        let mut probe = JavaRandom::new(seed);
        let _ = probe.next_double();
        let roll = probe.next_int_bound(3);
        let context = TurnContext {
            current_planet: Some(planet),
            current_sector: Some((planet, sector_id)),
            playing_rules: Some((false, false)),
            current_wave: 12,
        };
        let before = campaign
            .sector(planet, sector_id)
            .unwrap()
            .info
            .info
            .clone();
        let report = campaign.run_turn(
            &registry,
            &mut settings,
            &context,
            &mut JavaRandom::new(seed),
        );
        assert_eq!(
            report.events,
            vec![
                CampaignEvent::SectorInvasion {
                    planet,
                    sector: sector_id,
                    win_wave: 12 + (2 + roll) * 5,
                    played: true,
                },
                CampaignEvent::Turn,
            ]
        );
        // The played sector's record is untouched: the host writes the live
        // session rules (`Universe.java:550-553`).
        let after = campaign
            .sector(planet, sector_id)
            .unwrap()
            .info
            .info
            .clone();
        assert_eq!(after.win_wave, before.win_wave);
        assert_eq!(after.waves, before.waves);
        assert_eq!(after.attack, before.attack);
    }

    #[test]
    fn invasion_of_off_play_sector_writes_the_record() {
        let (registry, mut campaign, planet) = serpulo();
        let mut settings = SettingsStore::new();
        let (sector_id, _neighbor_id) = invasion_setup(&mut campaign, planet);
        let seed = (0..10_000u64)
            .find(|seed| JavaRandom::new(*seed).next_double() < 0.8 * BASE_INVASION_CHANCE)
            .expect("a seed in range");
        let mut probe = JavaRandom::new(seed);
        let _ = probe.next_double();
        let roll = probe.next_int_bound(3);
        // `max(info.winWave, info.wave) + (2 + r) * 5`.
        let expected = 10 + (2 + roll) * 5;
        let report = campaign.run_turn(
            &registry,
            &mut settings,
            &TurnContext::default(),
            &mut JavaRandom::new(seed),
        );
        assert_eq!(
            report.events,
            vec![
                CampaignEvent::SectorInvasion {
                    planet,
                    sector: sector_id,
                    win_wave: expected,
                    played: false,
                },
                CampaignEvent::Turn,
            ]
        );
        let record = campaign
            .sector(planet, sector_id)
            .unwrap()
            .info
            .info
            .clone();
        assert_eq!(record.win_wave, expected);
        assert!(record.waves);
        assert!(!record.attack);
        // `sector.saveInfo()` persisted the invaded record (`Universe.java:554`).
        let saved: crate::io::json::rules::SectorInfo = settings
            .get_json(&crate::game::sector::info_key("serpulo", sector_id))
            .expect("read settings")
            .expect("record present");
        assert!(saved.waves);
        assert_eq!(saved.win_wave, expected);
    }
}
