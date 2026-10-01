// SPDX-License-Identifier: GPL-3.0-only

//! Vanilla sector-preset registry.
//!
//! Ported from `core/src/mindustry/content/SectorPresets.java` (46 presets,
//! exact `load()` order), `core/src/mindustry/type/{SectorPreset,SectorDifficulty}.java`.
//! `SectorSubmissions.registerSectors()` (plan 19 map submissions) is behind a
//! no-op hook; the planet-data remap is identity until plan 12 (`SectorRemapProvider`).

use super::super::bundle::BundleView;
use super::super::color::Rgba;
use super::super::ctype::{Content, Mappable, ModContentInfo, UnlockFields, Unlockable};
use super::super::id::{PlanetId, SectorId};
use super::super::load::ContentRegistry;
use super::super::settings_store::UnlockStore;
use super::super::{ContentError, ContentType};

/// `SectorDifficulty` lower bounds (`SectorDifficulty.java`).
pub mod sector_difficulty {
    /// `low`.
    pub const LOW: i32 = 0;
    /// `medium`.
    pub const MEDIUM: i32 = 3;
    /// `high`.
    pub const HIGH: i32 = 5;
    /// `extreme`.
    pub const EXTREME: i32 = 8;
    /// `eradication`.
    pub const ERADICATION: i32 = 10;
    /// `unreasonable`.
    pub const UNREASONABLE: i32 = 13;
}

/// Planet-data preset remap (`Planet.getData().presets`); identity until plan 12.
pub trait SectorRemapProvider {
    /// Maps a preset name to a sector index for `planet` (`fallback` = authored index).
    fn preset_remap(&self, _planet: PlanetId, _name: &str, fallback: i32) -> i32 {
        fallback
    }
}

/// Default identity provider.
#[derive(Debug, Clone, Copy, Default)]
pub struct IdentityRemap;

impl SectorRemapProvider for IdentityRemap {}

/// Rules override kind tag (`Cons<Rules>`; values in plan 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleOverrideKind {
    /// Default `rules -> rules.winWave = captureWave`.
    Default,
    /// No per-sector overrides above the default.
    None,
}

/// Sector preset record (`mindustry.type.SectorPreset`).
#[derive(Debug, Clone, PartialEq)]
pub struct SectorPresetDef {
    /// Dense id in the sector content space.
    pub id: SectorId,
    /// Content name (parity ABI).
    pub name: String,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// Unlock/database fields.
    pub unlock: UnlockFields,
    /// Planet this preset belongs to.
    pub planet: PlanetId,
    /// Resolved sector index after remap/modulo.
    pub sector: u16,
    /// Authored sector index (`originalPosition`).
    pub original_position: i32,
    /// Wave at which the sector is captured (`0` = never).
    pub capture_wave: i32,
    /// Difficulty, 0-10.
    pub difficulty: f32,
    /// Wave time multiplier on start.
    pub start_wave_time_multiplier: f32,
    /// Adds starting items.
    pub add_starting_items: bool,
    /// Disables lighting.
    pub no_lighting: bool,
    /// Last sector in its planetary campaign.
    pub is_last_sector: bool,
    /// Must be unlocked before landing.
    pub require_unlock: bool,
    /// Shows icon/name even when hidden.
    pub show_hidden: bool,
    /// Uses this sector's launch fields.
    pub override_launch_defaults: bool,
    /// Launch-schematic allowance.
    pub allow_launch_schematics: bool,
    /// Launch-loadout allowance.
    pub allow_launch_loadout: bool,
    /// Switches to attack mode after waves.
    pub attack_after_waves: bool,
    /// Sectors that must be completed before landing here.
    pub shield_sectors: Vec<u16>,
    /// Rules override kind.
    pub rules: RuleOverrideKind,
    /// Outline generation for the `sector-<name>` icon.
    pub outline: bool,
    /// Outline radius.
    pub outline_radius: i32,
    /// Outline color.
    pub outline_color: Rgba,
    /// Optional map file name override.
    pub file_name: Option<String>,
}

impl SectorPresetDef {
    /// Creates a preset assigned to `planet` at authored `sector` index.
    pub fn new(
        name: &str,
        planet: PlanetId,
        sector: i32,
        bundle: &dyn BundleView,
        store: &dyn UnlockStore,
    ) -> Self {
        Self {
            id: SectorId::new(0),
            name: name.to_owned(),
            minfo: ModContentInfo::default(),
            removed: false,
            unlock: UnlockFields::new(ContentType::Sector, name, bundle, store),
            planet,
            sector: 0,
            original_position: sector,
            capture_wave: 0,
            difficulty: 0.0,
            start_wave_time_multiplier: 2.0,
            add_starting_items: false,
            no_lighting: false,
            is_last_sector: false,
            require_unlock: true,
            show_hidden: false,
            override_launch_defaults: false,
            allow_launch_schematics: false,
            allow_launch_loadout: false,
            attack_after_waves: false,
            shield_sectors: Vec::new(),
            rules: RuleOverrideKind::Default,
            outline: true,
            outline_radius: 5,
            outline_color: Rgba::new(0.5, 0.5, 0.5, 1.0),
            file_name: None,
        }
    }

    /// `SectorPreset.isHidden()`: hidden when no description exists.
    pub fn is_hidden(&self) -> bool {
        self.unlock.description.is_none()
    }
}

impl Content for SectorPresetDef {
    const TYPE: ContentType = ContentType::Sector;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = SectorId::new(id);
    }

    fn minfo(&self) -> &ModContentInfo {
        &self.minfo
    }

    fn minfo_mut(&mut self) -> &mut ModContentInfo {
        &mut self.minfo
    }

    fn removed(&self) -> bool {
        self.removed
    }

    fn set_removed(&mut self, removed: bool) {
        self.removed = removed;
    }

    fn kind_name(&self) -> &'static str {
        "SectorPreset"
    }

    fn content_name(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn unlock_fields(&self) -> Option<&UnlockFields> {
        Some(&self.unlock)
    }

    fn init_self(&mut self) -> Result<(), ContentError> {
        // `SectorPreset.init` assigns shield targets once all presets exist; the
        // registry `link()` pass does the cross-content half.
        Ok(())
    }

    fn post_init(&mut self) -> Result<(), ContentError> {
        self.unlock.post_init();
        Ok(())
    }
}

impl Mappable for SectorPresetDef {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Unlockable for SectorPresetDef {
    fn unlock(&self) -> &UnlockFields {
        &self.unlock
    }

    fn unlock_mut(&mut self) -> &mut UnlockFields {
        &mut self.unlock
    }
}

/// `SectorSubmissions.registerSectors()` hook (plan 19 map submissions).
pub fn register_sector_submissions(_registry: &mut ContentRegistry) {
    // Intentionally empty until plan 19 lands map-submission sectors.
}

/// Resolves one preset's planet/sector and assigns `planet.sectors[i].preset`
/// (`SectorPreset.initialize`).
pub fn resolve_preset(
    registry: &mut ContentRegistry,
    preset: SectorId,
    remap: &dyn SectorRemapProvider,
) -> Result<(), ContentError> {
    let (planet, original) = {
        let record = registry
            .sectors()
            .get(preset.index())
            .ok_or(ContentError::UnknownId(preset.raw()))?;
        (record.planet, record.original_position)
    };
    let name = registry
        .sector(preset)
        .map(|record| record.name.clone())
        .unwrap_or_default();
    let sector_count = registry
        .planet(planet)
        .map(|record| record.sector_count)
        .ok_or(ContentError::UnknownId(planet.raw()))?;
    if sector_count == 0 {
        return Err(ContentError::Parse(format!(
            "preset '{name}' assigned to a planet without sectors"
        )));
    }
    let remapped = remap.preset_remap(planet, &name, original);
    let mut index = remapped % sector_count as i32;
    if index == -1 {
        index = 0;
    }
    let index = index.clamp(0, sector_count as i32 - 1) as usize;
    if let Some(record) = registry.sector_mut(preset) {
        record.sector = index as u16;
    }
    if let Some(planet_record) = registry.planet_mut(planet) {
        planet_record.set_preset(index, preset);
    }
    Ok(())
}

/// `SectorPreset.init`: propagates shield targets (`sectors can only have one
/// visual shield target`).
pub(crate) fn link(registry: &mut ContentRegistry) -> Result<(), ContentError> {
    let links: Vec<(PlanetId, u16, Vec<u16>)> = registry
        .sectors()
        .iter()
        .filter(|preset| !preset.shield_sectors.is_empty())
        .map(|preset| (preset.planet, preset.sector, preset.shield_sectors.clone()))
        .collect();
    for (planet_id, target, shields) in links {
        if let Some(planet) = registry.planet_mut(planet_id) {
            for shield in shields {
                if let Some(sector) = planet.sectors.get_mut(shield as usize) {
                    sector.shield_target = Some(target);
                }
            }
        }
    }
    Ok(())
}

/// Loads all 46 vanilla presets in `SectorPresets.load()` order.
pub fn load(
    registry: &mut ContentRegistry,
    bundle: &dyn BundleView,
    store: &dyn UnlockStore,
    remap: &dyn SectorRemapProvider,
) -> Result<(), ContentError> {
    let planet_id = |registry: &ContentRegistry, name: &str| {
        registry.planet_by_name(name).map(|planet| planet.id)
    };
    let serpulo = planet_id(registry, "serpulo");
    let erekir = planet_id(registry, "erekir");
    let sun = planet_id(registry, "sun");
    let default_planet = serpulo
        .or(erekir)
        .or(sun)
        .ok_or_else(|| ContentError::Parse(String::from("sector presets require planets")))?;
    let mut ids = Vec::with_capacity(46);

    macro_rules! preset {
        ($name:literal, $planet:expr, $sector:expr, $configure:expr) => {{
            let mut preset = SectorPresetDef::new($name, $planet, $sector, bundle, store);
            #[allow(unused_mut)]
            let mut configure = $configure;
            configure(&mut preset);
            ids.push(registry.add_sector(preset)?);
        }};
    }

    // region serpulo
    preset!(
        "groundZero",
        serpulo.unwrap_or(default_planet),
        15,
        |preset: &mut SectorPresetDef| {
            preset.unlock.always_unlocked = true;
            preset.add_starting_items = true;
            preset.capture_wave = 10;
            preset.difficulty = 1.0;
            preset.override_launch_defaults = true;
            preset.no_lighting = true;
            preset.start_wave_time_multiplier = 3.0;
        }
    );
    preset!(
        "saltFlats",
        serpulo.unwrap_or(default_planet),
        101,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 5.0;
        }
    );
    preset!(
        "testingGrounds",
        serpulo.unwrap_or(default_planet),
        3,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 7.0;
            preset.capture_wave = 33;
        }
    );
    preset!(
        "frozenForest",
        serpulo.unwrap_or(default_planet),
        86,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 15;
            preset.difficulty = 2.0;
        }
    );
    preset!(
        "biomassFacility",
        serpulo.unwrap_or(default_planet),
        81,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 20;
            preset.difficulty = 3.0;
        }
    );
    preset!(
        "taintedWoods",
        serpulo.unwrap_or(default_planet),
        221,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 33;
            preset.difficulty = 5.0;
        }
    );
    preset!(
        "crateredBattleground",
        serpulo.unwrap_or(default_planet),
        18,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 20;
            preset.difficulty = 2.0;
        }
    );
    preset!(
        "ruinousShores",
        serpulo.unwrap_or(default_planet),
        213,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 30;
            preset.difficulty = 3.0;
        }
    );
    preset!(
        "perilousHarbor",
        serpulo.unwrap_or(default_planet),
        47,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 4.0;
        }
    );
    preset!(
        "facility32m",
        serpulo.unwrap_or(default_planet),
        64,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 25;
            preset.difficulty = 4.0;
        }
    );
    preset!(
        "windsweptIslands",
        serpulo.unwrap_or(default_planet),
        246,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 30;
            preset.difficulty = 4.0;
        }
    );
    preset!(
        "stainedMountains",
        serpulo.unwrap_or(default_planet),
        20,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 30;
            preset.difficulty = 3.0;
        }
    );
    preset!(
        "extractionOutpost",
        serpulo.unwrap_or(default_planet),
        165,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 5.0;
        }
    );
    preset!(
        "coastline",
        serpulo.unwrap_or(default_planet),
        108,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 30;
            preset.difficulty = 5.0;
        }
    );
    preset!(
        "weatheredChannels",
        serpulo.unwrap_or(default_planet),
        39,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 40;
            preset.difficulty = 9.0;
        }
    );
    preset!(
        "navalFortress",
        serpulo.unwrap_or(default_planet),
        216,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 8.0;
        }
    );
    preset!(
        "frontier",
        serpulo.unwrap_or(default_planet),
        50,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 4.0;
        }
    );
    preset!(
        "fungalPass",
        serpulo.unwrap_or(default_planet),
        21,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 2.0;
        }
    );
    preset!(
        "infestedCanyons",
        serpulo.unwrap_or(default_planet),
        210,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 4.0;
        }
    );
    preset!(
        "atolls",
        serpulo.unwrap_or(default_planet),
        1,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 7.0;
        }
    );
    preset!(
        "sunkenPier",
        serpulo.unwrap_or(default_planet),
        -1,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 50;
            preset.difficulty = 8.0;
        }
    );
    preset!(
        "mycelialBastion",
        serpulo.unwrap_or(default_planet),
        260,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 8.0;
        }
    );
    preset!(
        "overgrowth",
        serpulo.unwrap_or(default_planet),
        134,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 5.0;
        }
    );
    preset!(
        "tarFields",
        serpulo.unwrap_or(default_planet),
        23,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 40;
            preset.difficulty = 5.0;
        }
    );
    preset!(
        "impact0078",
        serpulo.unwrap_or(default_planet),
        227,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 45;
            preset.difficulty = 7.0;
        }
    );
    preset!(
        "desolateRift",
        serpulo.unwrap_or(default_planet),
        123,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 18;
            preset.difficulty = 8.0;
        }
    );
    preset!(
        "nuclearComplex",
        serpulo.unwrap_or(default_planet),
        130,
        |preset: &mut SectorPresetDef| {
            preset.capture_wave = 50;
            preset.difficulty = 7.0;
        }
    );
    preset!(
        "littoralShipyard",
        serpulo.unwrap_or(default_planet),
        204,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 9.0;
        }
    );
    preset!(
        "planetaryTerminal",
        serpulo.unwrap_or(default_planet),
        93,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 10.0;
            preset.is_last_sector = true;
        }
    );

    register_sector_submissions(registry);
    // endregion

    // region erekir
    preset!(
        "onset",
        erekir.unwrap_or(default_planet),
        10,
        |preset: &mut SectorPresetDef| {
            preset.unlock.always_unlocked = true;
            preset.difficulty = 1.0;
        }
    );
    preset!(
        "aegis",
        erekir.unwrap_or(default_planet),
        88,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 3.0;
        }
    );
    preset!(
        "lake",
        erekir.unwrap_or(default_planet),
        41,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 4.0;
        }
    );
    preset!(
        "intersect",
        erekir.unwrap_or(default_planet),
        36,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 5.0;
            preset.capture_wave = 9;
            preset.attack_after_waves = true;
        }
    );
    preset!(
        "atlas",
        erekir.unwrap_or(default_planet),
        14,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 5.0;
        }
    );
    preset!(
        "split",
        erekir.unwrap_or(default_planet),
        19,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 2.0;
        }
    );
    preset!(
        "basin",
        erekir.unwrap_or(default_planet),
        29,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 6.0;
        }
    );
    preset!(
        "marsh",
        erekir.unwrap_or(default_planet),
        25,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 4.0;
        }
    );
    preset!(
        "peaks",
        erekir.unwrap_or(default_planet),
        30,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 3.0;
        }
    );
    preset!(
        "ravine",
        erekir.unwrap_or(default_planet),
        39,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 4.0;
            preset.capture_wave = 24;
        }
    );
    preset!(
        "caldera-erekir",
        erekir.unwrap_or(default_planet),
        43,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 4.0;
        }
    );
    preset!(
        "stronghold",
        erekir.unwrap_or(default_planet),
        18,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 7.0;
        }
    );
    preset!(
        "crevice",
        erekir.unwrap_or(default_planet),
        3,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 6.0;
            preset.capture_wave = 46;
        }
    );
    preset!(
        "siege",
        erekir.unwrap_or(default_planet),
        58,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 8.0;
        }
    );
    preset!(
        "crossroads",
        erekir.unwrap_or(default_planet),
        37,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 7.0;
        }
    );
    preset!(
        "karst",
        erekir.unwrap_or(default_planet),
        5,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 9.0;
            preset.capture_wave = 10;
        }
    );
    preset!(
        "origin",
        erekir.unwrap_or(default_planet),
        12,
        |preset: &mut SectorPresetDef| {
            preset.difficulty = 10.0;
            preset.is_last_sector = true;
        }
    );
    // endregion

    for id in ids {
        resolve_preset(registry, id, remap)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::test_registry;
    use super::*;

    /// `sectors::presets_resolve` (plan 02 §5 M2 / `ApplicationTests.testSectorValidity`).
    #[test]
    fn presets_resolve() {
        let registry = test_registry();
        assert_eq!(registry.sectors().len(), 46, "preset count");
        for preset in registry.sectors() {
            let planet = registry
                .planet(preset.planet)
                .unwrap_or_else(|| panic!("{}: planet missing", preset.name));
            assert!(planet.sector_count > 0, "{}: no sectors", preset.name);
            assert!(
                (preset.sector as usize) < planet.sector_count,
                "{}: sector index {} out of range {}",
                preset.name,
                preset.sector,
                planet.sector_count
            );
            let sector = &planet.sectors[preset.sector as usize];
            assert_eq!(
                sector.preset,
                Some(preset.id),
                "{}: planet.sector round-trip",
                preset.name
            );
        }
        // Negative authored index wraps like Java `%` + `== -1 -> 0`.
        let sunken = registry.sector_by_name("sunkenPier").unwrap();
        assert_eq!(sunken.original_position, -1);
        assert_eq!(sunken.sector, 0);
        // Difficulty constants.
        assert_eq!(sector_difficulty::ERADICATION, 10);
        // Hidden rule: no description -> hidden (empty test bundle).
        assert!(sunken.is_hidden());
    }
}
