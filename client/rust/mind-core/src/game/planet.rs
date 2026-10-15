// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Planet runtime (plan 12 M3).
//!
//! Ported from the runtime half of `core/src/mindustry/type/Planet.java`:
//! positions/orbits, campaign-rules/stats persistence, last sector, base
//! coverage and `applyRules`. The static content record lives in
//! [`crate::content::registries::planets::PlanetDef`] (plan 02).

use indexmap::IndexMap;

use super::campaign_rules::CampaignRules;
use super::rules::Rules;
use super::sector::Sector;
use super::stats::CampaignStats;
use crate::content::registries::planets::{EnvFlag, PlanetDef};
use crate::content::{ContentRegistry, PlanetId};
use crate::io::json::rules::Vec3;
use crate::io::settings::SettingsStore;
use crate::random::JavaRandom;

/// `Time.toMinutes` in ticks (`60 * 60`) for the default rotation period.
const ROTATE_TIME: f32 = 24.0 * 60.0 * 60.0;

/// Sector adjacency seam (`PlanetGrid` is plan 06).
///
/// The vanilla icosphere grid is not on this branch; the campaign builder
/// supplies neighbor lists from a plan-06 implementation, or the empty default
/// (no adjacency) keeps `update_base_coverage`/invasion counting deterministic.
pub trait SectorNeighborhood {
    /// Neighbor sector ids of `sector` on `planet`.
    fn near(&self, planet: PlanetId, sector: u16) -> Vec<u16> {
        let _ = (planet, sector);
        Vec::new()
    }
}

/// No-adjacency default.
#[derive(Debug, Clone, Copy, Default)]
pub struct EmptyNeighborhood;

impl SectorNeighborhood for EmptyNeighborhood {}

/// Planet runtime state.
#[derive(Debug, Clone, PartialEq)]
pub struct Planet {
    /// Dense planet id (content ABI).
    pub id: PlanetId,
    /// Content name (settings keys/JSON).
    pub name: String,
    /// Parent body (`None` = solar-system center).
    pub parent: Option<PlanetId>,
    /// Direct children.
    pub children: Vec<PlanetId>,
    /// Sphere radius.
    pub radius: f32,
    /// Sector-grid subdivision (`0` = no landable grid).
    pub sector_tiles: u8,
    /// Orbit spacing defined by the parent.
    pub orbit_spacing: f32,
    /// Random orbit-angle offset.
    pub orbit_offset: f32,
    /// Orbital radius around the parent.
    pub orbit_radius: f32,
    /// Seconds per full orbit (`Kepler`-derived).
    pub orbit_time: f32,
    /// Seconds per full revolution.
    pub rotate_time: f32,
    /// Radius including children.
    pub total_radius: f32,
    /// Tidally locked to the parent.
    pub tidal_lock: bool,
    /// Global position (updated by `Universe::update_global`).
    pub position: Vec3,
    /// Global campaign modifiers.
    pub campaign_rules: CampaignRules,
    /// Defaults copied into `campaign_rules` at init.
    pub campaign_rule_defaults: CampaignRules,
    /// Lifetime campaign stats.
    pub stats: CampaignStats,
    /// Planet that shares this planet's stats (`statParent`).
    pub stat_parent: Option<PlanetId>,
    /// Last selected sector index.
    pub last_sector: u16,
    /// Simulated day/night cycle.
    pub update_lighting: bool,
    /// Day/night light range source.
    pub light_src_from: f32,
    /// Day/night light range source max.
    pub light_src_to: f32,
    /// Day/night light range destination.
    pub light_dst_from: f32,
    /// Day/night light range destination max.
    pub light_dst_to: f32,
    /// RTS-AI rule is customizable.
    pub show_rts_ai_rule: bool,
    /// Waves on sector loss.
    pub allow_waves: bool,
    /// Launch-loadout picker allowed on this planet.
    pub allow_launch_loadout: bool,
    /// Launch-schematic picker allowed on this planet.
    pub allow_launch_schematics: bool,
    /// Default start sector.
    pub start_sector: u16,
    /// Simulate sector invasions.
    pub allow_sector_invasion: bool,
    /// `Planet.ruleSetter` kind (`Planet.applyRules`, `Planets.java:74-88`).
    pub rule_setter: crate::content::registries::planets::RuleSetterKind,
    /// `Planet.enemyInfiniteItems` (default true; drives the wave team's
    /// `infiniteResources`/`fillItems` on the world-load event).
    pub enemy_infinite_items: bool,
    /// `Planet.enemyBuildSpeedMultiplier` (Erekir 0.4).
    pub enemy_build_speed_multiplier: f32,
    /// `Planet.enemyFactoryActivationDelay` (Serpulo 7200).
    pub enemy_factory_activation_delay: f32,
    /// Environment bitmask (`Env`).
    pub default_env: i32,
    /// Default environment attributes.
    pub default_attributes: Vec<(crate::content::Attribute, f32)>,
    /// Runtime sectors.
    pub sectors: Vec<Sector>,
    /// Sector adjacency (plan 06 seam).
    pub neighbors: Vec<Vec<u16>>,
}

/// `Env` bit for a flag (`mindustry.world.meta.Env`, `Env.java:5-26`).
/// This is the only `Env` bit table in `mind-core`; every env mask
/// (`Rules.env`, `Block`/`UnitType` `envEnabled/envDisabled/envRequired`)
/// is built from it.
pub const fn env_bit(flag: EnvFlag) -> i32 {
    match flag {
        EnvFlag::Terrestrial => 1,
        EnvFlag::Space => 1 << 1,
        EnvFlag::Underwater => 1 << 2,
        EnvFlag::Spores => 1 << 3,
        EnvFlag::Scorching => 1 << 4,
        EnvFlag::GroundOil => 1 << 5,
        EnvFlag::GroundWater => 1 << 6,
        EnvFlag::Oxygen => 1 << 7,
    }
}

/// Folds a `PlanetDef.default_env` list into an `Env` bitmask.
pub fn env_mask(flags: &[EnvFlag]) -> i32 {
    flags.iter().fold(0, |mask, flag| mask | env_bit(*flag))
}

impl Planet {
    /// Builds the runtime from a content record and its parent's total radius.
    pub fn from_def(registry: &ContentRegistry, def: &PlanetDef, parent_total_radius: f32) -> Self {
        let orbit_offset = JavaRandom::new(def.id.raw() as u64 + 1).next_int_bound(360) as f32;
        let total_radius = def.total_radius;
        let orbit_radius = if def.parent.is_none() {
            0.0
        } else {
            parent_total_radius + def.orbit_spacing + total_radius
        };
        let orbit_time = orbit_radius.max(0.0).powf(1.5) * 1000.0;

        let mut sectors = Vec::with_capacity(def.sectors.len());
        let mut neighbors = Vec::with_capacity(def.sectors.len());
        for content_sector in &def.sectors {
            let mut sector = Sector::new(def.id, content_sector.id);
            sector.threat = content_sector.threat;
            sector.generate_enemy_base = content_sector.generate_enemy_base;
            sector.shield_target = content_sector.shield_target;
            if let Some(preset_id) = content_sector.preset
                && let Some(preset) = registry.sector(preset_id)
            {
                sector.preset = Some(preset_id);
                sector.preset_difficulty = preset.difficulty;
                sector.preset_require_unlock = preset.require_unlock;
                sector.preset_capture_wave = preset.capture_wave;
                sector.preset_always_unlocked = preset.unlock.always_unlocked;
                sector.preset_name = Some(preset.name.clone());
                sector.preset_start_wave_time_multiplier = preset.start_wave_time_multiplier;
                sector.preset_add_starting_items = preset.add_starting_items;
                sector.preset_no_lighting = preset.no_lighting;
                sector.preset_is_last_sector = preset.is_last_sector;
                sector.preset_attack_after_waves = preset.attack_after_waves;
                sector.preset_override_launch_defaults = preset.override_launch_defaults;
                sector.preset_allow_launch_loadout = preset.allow_launch_loadout;
                sector.preset_allow_launch_schematics = preset.allow_launch_schematics;
            }
            sectors.push(sector);
            neighbors.push(Vec::new());
        }

        // `Planet.init`: applyDefaultRules(campaignRules) copies the assigned
        // defaults into the live rules, then `loadRules` overrides from settings.
        // Java `Planet.applyDefaultRules` (`Planet.java:308-309`) yields
        // `CampaignRules` all-false except Erekir's fog/hideSpawns/rtsAI
        // (`CampaignRules.java:7-16`); Serpulo's `allowSectorInvasion` /
        // `allowLegacyLaunchPads` only gate whether the player may toggle them,
        // they are not the defaults.
        let defaults = CampaignRules {
            fog: def.campaign_rule_defaults.fog,
            hide_spawns: def.campaign_rule_defaults.hide_spawns,
            rts_ai: def.campaign_rule_defaults.rts_ai,
            sector_invasion: false,
            legacy_launch_pads: false,
            clear_sector_on_lose: def.clear_sector_on_lose,
            ..CampaignRules::default()
        };
        let campaign_rules = defaults.clone();

        Self {
            id: def.id,
            name: def.name.clone(),
            parent: def.parent,
            children: Vec::new(),
            radius: def.radius,
            sector_tiles: def.sector_tiles,
            orbit_spacing: def.orbit_spacing,
            orbit_offset,
            orbit_radius,
            orbit_time,
            rotate_time: ROTATE_TIME,
            total_radius,
            tidal_lock: def.tidal_lock,
            position: Vec3::default(),
            campaign_rules,
            campaign_rule_defaults: defaults,
            stats: CampaignStats::new(),
            stat_parent: None,
            last_sector: def.start_sector as u16,
            update_lighting: def.update_lighting,
            light_src_from: 0.0,
            light_src_to: def.light_src_to,
            light_dst_from: def.light_dst_from,
            light_dst_to: 1.0,
            show_rts_ai_rule: def.show_rts_ai_rule,
            allow_waves: def.allow_waves,
            allow_launch_loadout: def.allow_launch_loadout,
            allow_launch_schematics: def.allow_launch_schematics,
            start_sector: def.start_sector as u16,
            allow_sector_invasion: def.allow_sector_invasion,
            rule_setter: def.rule_setter,
            enemy_infinite_items: def.enemy_infinite_items,
            enemy_build_speed_multiplier: def.enemy_build_speed_multiplier,
            enemy_factory_activation_delay: def.enemy_factory_activation_delay,
            default_env: env_mask(&def.default_env),
            default_attributes: def.default_attributes.clone(),
            sectors,
            neighbors,
        }
    }

    /// Sector by index.
    pub fn sector(&self, id: u16) -> Option<&Sector> {
        self.sectors.get(id as usize)
    }

    /// Mutable sector by index.
    pub fn sector_mut(&mut self, id: u16) -> Option<&mut Sector> {
        self.sectors.get_mut(id as usize)
    }

    /// Sector count.
    pub fn sector_count(&self) -> usize {
        self.sectors.len()
    }

    /// `Planet.hasGrid`.
    pub fn has_grid(&self) -> bool {
        !self.sectors.is_empty()
    }

    /// `Planet.isLandable`.
    pub fn is_landable(&self) -> bool {
        !self.sectors.is_empty()
    }

    /// `Planet.getOrbitAngle` (`universe.secondsf()` supplied by the caller).
    pub fn get_orbit_angle(&self, seconds: f32) -> f32 {
        (self.orbit_offset + seconds / (self.orbit_time / 360.0)) % 360.0
    }

    /// `Planet.getRotation`.
    pub fn get_rotation(&self, seconds: f32) -> f32 {
        if self.tidal_lock {
            return -self.get_orbit_angle(seconds) + 90.0;
        }
        let offset = JavaRandom::new(self.id.raw() as u64 + 1).next_int_bound(360) as f32;
        (offset + seconds / (self.rotate_time / 360.0)) % 360.0
    }

    /// `Planet.addParentOffset`.
    pub fn add_parent_offset(&self, in_: Vec3, seconds: f32) -> Vec3 {
        if self.parent.is_none() || self.orbit_radius == 0.0 {
            return in_;
        }
        let angle = self.get_orbit_angle(seconds);
        let radians = angle.to_radians();
        Vec3 {
            x: in_.x + radians.sin() * self.orbit_radius,
            y: in_.y,
            z: in_.z + radians.cos() * self.orbit_radius,
        }
    }

    /// `Planet.getWorldPosition` (absolute position including all parents).
    pub fn world_position(&self, all: &IndexMap<u16, Planet>, seconds: f32) -> Vec3 {
        let mut position = Vec3::default();
        let mut current: Option<&Planet> = Some(self);
        while let Some(planet) = current {
            position = planet.add_parent_offset(position, seconds);
            current = planet.parent.and_then(|id| all.get(&id.raw()));
        }
        position
    }

    /// `Planet.saveRules`.
    pub fn save_rules(&self, settings: &mut SettingsStore) -> Result<(), crate::io::IoError> {
        settings.put_json(
            &format!("{}-campaign-rules", self.name),
            &self.campaign_rules,
        )
    }

    /// `Planet.loadRules`.
    pub fn load_rules(&mut self, settings: &SettingsStore) {
        let fallback = self.campaign_rules.clone();
        self.campaign_rules =
            settings.get_json_or(&format!("{}-campaign-rules", self.name), || fallback);
    }

    /// `Planet.stats` (`statParent` delegation is a `Campaign` concern).
    pub fn own_stats(&self) -> &CampaignStats {
        &self.stats
    }

    /// `Planet.loadStats`.
    pub fn load_stats(&mut self, settings: &SettingsStore) {
        if self.stat_parent.is_none() {
            self.stats =
                settings.get_json_or(&format!("{}-campaign-stats", self.name), CampaignStats::new);
        }
    }

    /// `Planet.saveStats` (single-planet; `statParent` handled by `Campaign`).
    pub fn save_stats(&self, settings: &mut SettingsStore) -> Result<(), crate::io::IoError> {
        settings.put_json(&format!("{}-campaign-stats", self.name), &self.stats)
    }

    /// `Planet.clearStats`: resets this planet's own lifetime stats.
    ///
    /// `statParent` sharing is a `Campaign` concern (`Campaign::stats_for_mut`
    /// resolves the shared record before calling this).
    pub fn clear_stats(&mut self) {
        self.stats = CampaignStats::new();
    }

    /// `Planet.getStartSector`.
    pub fn get_start_sector(&self) -> Option<&Sector> {
        if self.sectors.is_empty() {
            None
        } else {
            self.sectors.get(self.start_sector as usize)
        }
    }

    /// `Planet.getLastSector`.
    pub fn get_last_sector(&self, settings: &SettingsStore) -> Option<&Sector> {
        if self.sectors.is_empty() {
            return None;
        }
        let index = settings
            .get_i32(
                &format!("{}-last-sector", self.name),
                self.start_sector as i32,
            )
            .clamp(0, self.sectors.len() as i32 - 1) as usize;
        self.sectors.get(index)
    }

    /// `Planet.setLastSector`.
    pub fn set_last_sector(&mut self, settings: &mut SettingsStore, sector: u16) {
        settings.put_i32(&format!("{}-last-sector", self.name), sector as i32);
        self.last_sector = sector;
    }

    /// The planet-globe "new preset" reveal (`PlanetDialog.show`,
    /// `PlanetDialog.java:270-288`): every assigned preset on this planet that
    /// is unlocked, still hidden, requires unlock, is not always-unlocked and
    /// has no base. Marks `info.shown` on each collected sector (Java
    /// `preset.sector.info.shown = true` + `saveInfo()`, the save is the
    /// caller's job) and returns the sector ids in content order.
    pub fn new_presets(&mut self, registry: &ContentRegistry) -> Vec<u16> {
        let mut out = Vec::new();
        for def in registry.sectors() {
            if def.removed || def.planet != self.id {
                continue;
            }
            // `preset.sector.preset == preset`: the preset is assigned to this
            // cell (`SectorPreset.initialize` remap already applied).
            let Some(sector) = self.sectors.get(def.sector as usize) else {
                continue;
            };
            if sector.preset != Some(def.id) {
                continue;
            }
            let unlocked = super::tech_tree::content_unlocked(
                registry,
                crate::content::ContentRef::of(crate::content::ContentType::Sector, def.id),
            );
            if unlocked
                && !sector.preset_always_unlocked
                && !sector.info.info.shown
                && sector.preset_require_unlock
                && !sector.has_base()
            {
                out.push(sector.id);
            }
        }
        for id in &out {
            if let Some(sector) = self.sectors.get_mut(*id as usize) {
                sector.info.info.shown = true;
            }
        }
        out
    }

    /// `Planet.applyRules(rules, customGame)`.
    pub fn apply_rules(
        &self,
        registry: &ContentRegistry,
        rules: &mut Rules,
        custom_game: bool,
        is_game: bool,
    ) -> super::campaign_rules::CampaignApply {
        let _ = registry;
        // `Planet.applyRules` (`Planet.java:295-306`): the planet's
        // `ruleSetter` runs first (`Planets.java:74-88`).
        match self.rule_setter {
            crate::content::registries::planets::RuleSetterKind::Serpulo => {
                rules.wave_team = 2;
                rules.place_range_check = false;
                rules.hide_spawns = true;
                rules.derelict_repair = true;
                rules.core_destroy_clear = true;
            }
            crate::content::registries::planets::RuleSetterKind::Erekir => {
                rules.wave_team = 3;
                rules.place_range_check = false;
                rules.hide_spawns = false;
                rules.fog = true;
                rules.static_fog = true;
                rules.lighting = false;
                rules.core_destroy_clear = true;
                rules.only_deposit_core = true;
            }
            // Tantros is an empty lambda upstream; planets without a setter
            // keep the map/save rules as-is.
            crate::content::registries::planets::RuleSetterKind::None
            | crate::content::registries::planets::RuleSetterKind::Tantros => {}
        }
        rules.attributes.0.clear();
        for (attribute, value) in &self.default_attributes {
            rules.attributes.set(*attribute, *value);
        }
        rules.env = self.default_env;
        rules.planet = self.name.clone();

        if custom_game {
            super::campaign_rules::CampaignApply::default()
        } else {
            self.campaign_rules.apply_planet(self, rules, is_game)
        }
    }

    /// `Planet.applyDefaultRules` (JSON copy of `campaignRuleDefaults`).
    pub fn apply_default_rules(&mut self) {
        // Plan 02 stores the defaults subset on the content record; the runtime
        // already captured them in `from_def`. `campaign_rule_defaults` remains
        // available for mods that call this explicitly.
        let defaults = self.campaign_rule_defaults.clone();
        self.campaign_rules = defaults;
    }

    /// `Planet.updateBaseCoverage`.
    pub fn update_base_coverage(&mut self, neighborhood: &dyn SectorNeighborhood) {
        for index in 0..self.sectors.len() {
            let id = self.sectors[index].id;
            let mut sum = 1.0f32;
            for neighbor in neighborhood.near(self.id, id) {
                if self
                    .sectors
                    .get(neighbor as usize)
                    .is_some_and(|s| s.generate_enemy_base)
                {
                    sum += 0.9;
                }
            }
            if self.sectors[index].has_enemy_base() {
                sum += 0.88;
            }
            let threat = if self.sectors[index].preset.is_none()
                || (!self.sectors[index].preset_require_unlock
                    && self.sectors[index].preset_difficulty == 0.0)
            {
                (sum / 5.0).clamp(0.3, 1.2)
            } else {
                (self.sectors[index].preset_difficulty / 10.0).clamp(0.0, 1.0)
            };
            self.sectors[index].threat = threat;
        }
    }

    /// `Planet.updateBaseCoverage` over the runtime `neighbors` built by
    /// [`super::universe::Campaign::from_registry`].
    pub fn update_base_coverage_from_neighbors(&mut self) {
        for index in 0..self.sectors.len() {
            let id = self.sectors[index].id;
            let mut sum = 1.0f32;
            for neighbor in self.neighbors.get(id as usize).into_iter().flatten() {
                if self
                    .sectors
                    .get(*neighbor as usize)
                    .is_some_and(|s| s.generate_enemy_base)
                {
                    sum += 0.9;
                }
            }
            if self.sectors[index].has_enemy_base() {
                sum += 0.88;
            }
            let threat = if self.sectors[index].preset.is_none()
                || (!self.sectors[index].preset_require_unlock
                    && self.sectors[index].preset_difficulty == 0.0)
            {
                (sum / 5.0).clamp(0.3, 1.2)
            } else {
                (self.sectors[index].preset_difficulty / 10.0).clamp(0.0, 1.0)
            };
            self.sectors[index].threat = threat;
        }
    }
}

/// `Planet.updateBaseCoverage` for an unsaved generated sector (H05 T2): the
/// campaign facade primes `MindSimHost` with the live values via
/// `prime_sector_load`; this static fallback mirrors what
/// [`Planet::update_base_coverage_from_neighbors`] would compute for the cell
/// before any campaign state is read (`has_base = false`).
pub fn static_gen_extras(
    registry: &ContentRegistry,
    planet: &str,
    sector: u16,
) -> crate::maps::planet::SectorGenExtras {
    let fallback = crate::maps::planet::SectorGenExtras::default();
    let Some(def) = registry.planet_by_name(planet) else {
        return fallback;
    };
    let grid = crate::render::g3d::grid::PlanetGrid::create(def.sector_tiles as usize);
    let Some(cell) = def.sectors.get(sector as usize) else {
        return fallback;
    };
    let mut sum = 1.0f32;
    for neighbor in grid
        .tiles
        .get(sector as usize)
        .map(|tile| &tile.tiles)
        .into_iter()
        .flatten()
    {
        if def
            .sectors
            .get(*neighbor)
            .is_some_and(|other| other.generate_enemy_base)
        {
            sum += 0.9;
        }
    }
    // `Sector.hasEnemyBase` with `save == null` (`Sector.java:144-146`).
    let has_enemy_base = cell.generate_enemy_base && cell.preset.is_none();
    if has_enemy_base {
        sum += 0.88;
    }
    let preset = cell.preset.and_then(|id| registry.sector(id));
    let threat = if cell.preset.is_none()
        || preset.is_some_and(|preset| !preset.require_unlock && preset.difficulty == 0.0)
    {
        (sum / 5.0).clamp(0.3, 1.2)
    } else {
        (preset.map_or(0.0, |preset| preset.difficulty) / 10.0).clamp(0.0, 1.0)
    };
    crate::maps::planet::SectorGenExtras {
        threat,
        has_enemy_base,
        has_base: false,
    }
}

/// Helper so `Planet::apply_rules` can call `CampaignRules::apply` without
/// rebuilding a `PlanetDef`. Mirrors `CampaignRules.apply` using the runtime
/// flags copied at construction.
impl CampaignRules {
    /// Runtime-flag variant of [`CampaignRules::apply`].
    pub fn apply_planet(
        &self,
        planet: &Planet,
        rules: &mut Rules,
        is_game: bool,
    ) -> super::campaign_rules::CampaignApply {
        rules.static_fog = self.fog;
        rules.fog = self.fog;
        rules.hide_spawns = self.hide_spawns;
        rules.random_wave_ai = self.random_wave_ai;
        rules.pause_disabled = self.pause_disabled;
        rules.objective_timer_multiplier = self.difficulty.wave_time_multiplier();

        let mut result = super::campaign_rules::CampaignApply::default();
        if planet.show_rts_ai_rule {
            let enabled = self.rts_ai && rules.attack_mode;
            let wave_team = rules.wave_team;
            let swapped = rules.team_rule(wave_team).rts_ai != enabled;
            let rule = rules.team_rule_mut(wave_team);
            rule.rts_ai = enabled;
            rule.rts_max_squad = 15;
            result.rts_enabled = enabled;
            result.rts_swapped = swapped && is_game;
        }
        let wave_team = rules.wave_team;
        let health = self.difficulty.enemy_health_multiplier();
        let spawn = self.difficulty.enemy_spawn_multiplier();
        let rule = rules.team_rule_mut(wave_team);
        rule.block_health_multiplier = health;
        rule.unit_health_multiplier = health;
        rule.unit_cost_multiplier = 1.0 / spawn;
        rule.unit_build_speed_multiplier = spawn;
        result
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};

    fn registry() -> ContentRegistry {
        create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).unwrap()
    }

    #[test]
    fn env_mask_bits_match_java_env() {
        assert_eq!(env_mask(&[EnvFlag::Terrestrial, EnvFlag::Oxygen]), 1 | 128);
        assert_eq!(env_mask(&[EnvFlag::Space]), 2);
        assert_eq!(env_bit(EnvFlag::Underwater), 4);
        assert_eq!(env_bit(EnvFlag::Spores), 8);
        assert_eq!(env_bit(EnvFlag::Scorching), 16);
        assert_eq!(env_bit(EnvFlag::GroundOil), 32);
        assert_eq!(env_bit(EnvFlag::GroundWater), 64);
    }

    /// Planet defaults (`Planets.java:56` erekir `scorching|terrestrial`,
    /// `:117` tantros `underwater|terrestrial`, `Planet.java:87` serpulo
    /// `terrestrial|spores|groundOil|groundWater|oxygen` = 233).
    #[test]
    fn planet_default_env_matches_java() {
        let registry = registry();
        let default_env = |name: &str| {
            let def = registry.planet_by_name(name).expect("planet");
            Planet::from_def(&registry, def, 0.0).default_env
        };
        assert_eq!(default_env("serpulo"), 233);
        assert_eq!(default_env("erekir"), 17);
        assert_eq!(default_env("tantros"), 5);
        assert_eq!(
            default_env("serpulo"),
            crate::io::json::rules::DEFAULT_ENV,
            "`Planet.defaultEnv` matches `Vars.defaultEnv`"
        );
    }

    #[test]
    fn from_def_copies_presets_and_orbits() {
        let registry = registry();
        let serpulo_def = registry.planet_by_name("serpulo").unwrap();
        let sun_def = registry.planet_by_name("sun").unwrap();
        let sun = Planet::from_def(&registry, sun_def, 0.0);
        assert_eq!(sun.start_sector, 0);
        let serpulo = Planet::from_def(&registry, serpulo_def, sun.total_radius);
        assert_eq!(serpulo.sector_count(), serpulo_def.sector_count);
        assert!(serpulo.orbit_radius > 0.0);
        // groundZero resolves to a preset on serpulo.
        let ground_zero = registry.sector_by_name("groundZero").unwrap();
        let sector = serpulo.sector(ground_zero.sector).unwrap();
        assert_eq!(sector.preset, Some(ground_zero.id));
        assert_eq!(sector.preset_difficulty, 1.0);
        assert_eq!(sector.preset_capture_wave, 10);
    }

    /// H04 T4: Java `Planet.applyDefaultRules` yields `CampaignRules` all-false
    /// except Erekir's fog/hideSpawns/rtsAI (`Planet.java:308-309`,
    /// `CampaignRules.java:7-16`); Serpulo's `allowSectorInvasion`/
    /// `allowLegacyLaunchPads` only gate player toggles, they are not defaults.
    #[test]
    fn serpulo_campaign_rule_defaults_match_java() {
        let registry = registry();
        let serpulo_def = registry.planet_by_name("serpulo").unwrap();
        let serpulo = Planet::from_def(&registry, serpulo_def, 0.0);
        assert!(!serpulo.campaign_rules.sector_invasion);
        assert!(!serpulo.campaign_rules.legacy_launch_pads);
        assert!(!serpulo.campaign_rules.fog);
        let erekir_def = registry.planet_by_name("erekir").unwrap();
        let erekir = Planet::from_def(&registry, erekir_def, 0.0);
        assert!(erekir.campaign_rules.fog, "erekir keeps its defaults");
        assert!(!erekir.campaign_rules.hide_spawns);
        assert!(erekir.campaign_rules.rts_ai);
    }

    #[test]
    fn apply_rules_sets_env_and_attributes() {
        let registry = registry();
        let erekir = registry.planet_by_name("erekir").unwrap();
        let planet = Planet::from_def(&registry, erekir, 0.0);
        let mut rules = Rules::default();
        rules.attack_mode = true;
        planet.apply_rules(&registry, &mut rules, false, false);
        assert_eq!(rules.planet, "erekir");
        assert!(rules.has_env(env_bit(EnvFlag::Scorching)));
        assert_eq!(rules.attributes.get(crate::content::Attribute::Heat), 0.8);
        // campaign defaults applied (erekir fog/rtsAi true).
        assert!(rules.fog);
    }

    /// T5: `Planet.applyRules` runs the planet's `ruleSetter`
    /// (`Planets.java:82-88` for serpulo, `74-83` for erekir).
    #[test]
    fn serpulo_rule_setter_applies() {
        let registry = registry();
        let serpulo_def = registry.planet_by_name("serpulo").unwrap();
        let serpulo = Planet::from_def(&registry, serpulo_def, 0.0);
        let mut rules = Rules::default();
        serpulo.apply_rules(&registry, &mut rules, true, false);
        assert_eq!(rules.wave_team, 2, "crux");
        assert!(!rules.place_range_check);
        assert!(rules.hide_spawns);
        assert!(rules.derelict_repair);
        assert!(rules.core_destroy_clear);
    }

    /// I04 T8: the globe reveal collects unlocked, hidden, `requireUnlock`,
    /// base-less presets in content order and marks them shown
    /// (`PlanetDialog.java:270-288`; `saveInfo` is the caller's job).
    #[test]
    fn new_presets_marks_shown() {
        let mut registry = registry();
        // Unlock one normal preset; `groundZero` is `alwaysUnlocked` and must
        // stay out of the reveal.
        let preset_id = registry.sector_by_name("frozenForest").unwrap().id;
        registry.sector_mut(preset_id).unwrap().unlock.unlocked = true;
        let serpulo_def = registry.planet_by_name("serpulo").unwrap();
        let mut serpulo = Planet::from_def(&registry, serpulo_def, 0.0);

        let first = serpulo.new_presets(&registry);
        let cell = registry.sector(preset_id).unwrap().sector;
        assert!(
            first.contains(&cell),
            "unlocked hidden preset is collected: {first:?}"
        );
        assert!(
            serpulo.sectors[cell as usize].info.info.shown,
            "collected presets are marked shown"
        );
        let ground_zero = registry.sector_by_name("groundZero").unwrap();
        assert!(
            !first.contains(&ground_zero.sector),
            "alwaysUnlocked presets are not collected"
        );

        let second = serpulo.new_presets(&registry);
        assert!(
            !second.contains(&cell),
            "a shown preset is not collected again"
        );
    }
}
