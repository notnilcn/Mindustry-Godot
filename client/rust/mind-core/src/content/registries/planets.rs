// SPDX-License-Identifier: GPL-3.0-only

//! Vanilla planet registry.
//!
//! Ported from `core/src/mindustry/content/Planets.java` (7 planets, exact
//! `load()` order), `core/src/mindustry/type/{Planet,Sector}.java` (metadata
//! half; generators/meshes/runtime in plans 06/12/16).

use super::super::bundle::BundleView;
use super::super::color::Rgba;
use super::super::ctype::{Content, Mappable, ModContentInfo, UnlockFields, Unlockable};
use super::super::id::{PlanetId, SectorId};
use super::super::load::ContentRegistry;
use super::super::settings_store::UnlockStore;
use super::super::tech::TreeId;
use super::super::{ContentError, ContentType};
use super::weathers::Attribute;

/// `PlanetGrid.tileCount(size) = 10 * 3^size + 2` (icosphere subdivision).
pub const fn sector_count_for(size: u8) -> usize {
    let mut count = 10usize;
    let mut index = 0;
    while index < size {
        count *= 3;
        index += 1;
    }
    count + 2
}

/// Planet generator kind tag (bodies in plan 06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GeneratorKind {
    /// No generator (space-only body).
    None,
    /// `SerpuloPlanetGenerator`.
    Serpulo,
    /// `ErekirPlanetGenerator`.
    Erekir,
    /// `TantrosPlanetGenerator`.
    Tantros,
    /// `AsteroidGenerator`.
    Asteroid,
    /// `FileMapGenerator` (planet presets/sectors).
    FileMap,
}

impl GeneratorKind {
    /// Java class name.
    pub const fn name(self) -> &'static str {
        match self {
            GeneratorKind::None => "None",
            GeneratorKind::Serpulo => "SerpuloPlanetGenerator",
            GeneratorKind::Erekir => "ErekirPlanetGenerator",
            GeneratorKind::Tantros => "TantrosPlanetGenerator",
            GeneratorKind::Asteroid => "AsteroidGenerator",
            GeneratorKind::FileMap => "FileMapGenerator",
        }
    }
}

/// Mesh loader kind tag (bodies in plan 16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeshKind {
    /// `ShaderSphereMesh`.
    ShaderSphere,
    /// `SunMesh`.
    Sun,
    /// `HexMesh(this, detail)`.
    Hex {
        /// Subdivision detail.
        detail: u8,
    },
    /// Asteroid noise mesh.
    Asteroid,
}

impl MeshKind {
    /// Java class name.
    pub const fn name(self) -> &'static str {
        match self {
            MeshKind::ShaderSphere => "ShaderSphereMesh",
            MeshKind::Sun => "SunMesh",
            MeshKind::Hex { .. } => "HexMesh",
            MeshKind::Asteroid => "NoiseMesh",
        }
    }
}

/// Cloud mesh loader kind tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CloudMeshKind {
    /// `MultiMesh(HexSkyMesh ..)`.
    MultiHex,
}

/// Rule-setter kind tag (`Planet.ruleSetter`; rules data in plan 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleSetterKind {
    /// Empty rule setter.
    None,
    /// Erekir campaign defaults.
    Erekir,
    /// Tantros (empty lambda upstream).
    Tantros,
    /// Serpulo campaign defaults.
    Serpulo,
}

impl RuleSetterKind {
    /// Java class-ish name.
    pub const fn name(self) -> &'static str {
        match self {
            RuleSetterKind::None => "None",
            RuleSetterKind::Erekir => "erekir",
            RuleSetterKind::Tantros => "tantros",
            RuleSetterKind::Serpulo => "serpulo",
        }
    }
}

/// Environment flag (`mindustry.world.meta.Env` name; bit assignment plan 06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EnvFlag {
    /// `Env.terrestrial`.
    Terrestrial,
    /// `Env.spores`.
    Spores,
    /// `Env.groundOil`.
    GroundOil,
    /// `Env.groundWater`.
    GroundWater,
    /// `Env.oxygen`.
    Oxygen,
    /// `Env.scorching`.
    Scorching,
    /// `Env.underwater`.
    Underwater,
    /// `Env.space`.
    Space,
}

impl EnvFlag {
    /// Java enum identifier.
    pub const fn name(self) -> &'static str {
        match self {
            EnvFlag::Terrestrial => "terrestrial",
            EnvFlag::Spores => "spores",
            EnvFlag::GroundOil => "groundOil",
            EnvFlag::GroundWater => "groundWater",
            EnvFlag::Oxygen => "oxygen",
            EnvFlag::Scorching => "scorching",
            EnvFlag::Underwater => "underwater",
            EnvFlag::Space => "space",
        }
    }
}

/// Campaign-rule defaults a planet overrides (`campaignRuleDefaults` subset).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CampaignRuleDefaults {
    /// Fog of war.
    pub fog: bool,
    /// Hide spawns.
    pub hide_spawns: bool,
    /// RTS AI.
    pub rts_ai: bool,
}

/// Explicitly-set `AsteroidGenerator` parameters (defaults in plan 06).
#[derive(Debug, Clone, PartialEq)]
pub struct AsteroidSpec {
    /// Base (untinted) block content name.
    pub base: String,
    /// Tint block content name.
    pub tint: String,
    /// Noise seed.
    pub seed: i32,
    /// Tint threshold.
    pub tint_thresh: f32,
    /// Number of mesh pieces.
    pub pieces: i32,
    /// Scale.
    pub scale: f32,
    /// Explicitly-set generator parameters as `(field, value)`.
    pub params: Vec<(&'static str, f32)>,
}

/// One planet-grid cell (`mindustry.type.Sector` content-time fields).
#[derive(Debug, Clone, PartialEq)]
pub struct Sector {
    /// Dense sector index in the planet grid.
    pub id: u16,
    /// Preset assigned to this sector, if any.
    pub preset: Option<SectorId>,
    /// Visual shield target sector index.
    pub shield_target: Option<u16>,
    /// Threat rating.
    pub threat: f32,
    /// Whether to generate an enemy base here.
    pub generate_enemy_base: bool,
}

impl Sector {
    /// Creates an unassigned sector.
    pub fn new(id: u16) -> Self {
        Self {
            id,
            preset: None,
            shield_target: None,
            threat: 0.0,
            generate_enemy_base: false,
        }
    }
}

/// Planet content record (`mindustry.type.Planet` metadata half).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanetDef {
    /// Dense id in the planet content space.
    pub id: PlanetId,
    /// Content name (parity ABI).
    pub name: String,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// Unlock/database fields.
    pub unlock: UnlockFields,
    /// Parent body (`None` = solar-system center).
    pub parent: Option<PlanetId>,
    /// Sphere radius.
    pub radius: f32,
    /// Sector-grid subdivision (`0` = no landable grid).
    pub sector_tiles: u8,
    /// Number of sectors in the grid (`PlanetGrid.tileCount`).
    pub sector_count: usize,
    /// Sector grid cells.
    pub sectors: Vec<Sector>,
    /// Generator kind tag.
    pub generator: GeneratorKind,
    /// Mesh kind tag.
    pub mesh: MeshKind,
    /// Cloud mesh kind tag.
    pub cloud_mesh: Option<CloudMeshKind>,
    /// Bloom effect enabled.
    pub bloom: bool,
    /// Listed in the planet access UI.
    pub accessible: bool,
    /// Displayed at all.
    pub visible: bool,
    /// Whether the planet has an atmosphere.
    pub has_atmosphere: bool,
    /// Simulated day/night cycle.
    pub update_lighting: bool,
    /// Tint of clouds shown when landing.
    pub land_cloud_color: Rgba,
    /// Sun light color (suns only).
    pub light_color: Rgba,
    /// Atmosphere tint.
    pub atmosphere_color: Rgba,
    /// Planet-list icon color.
    pub icon_color: Rgba,
    /// Environment flags for sectors.
    pub default_env: Vec<EnvFlag>,
    /// Environment attributes.
    pub default_attributes: Vec<(Attribute, f32)>,
    /// Rule setter kind tag.
    pub rule_setter: RuleSetterKind,
    /// Campaign-rule defaults override.
    pub campaign_rule_defaults: CampaignRuleDefaults,
    /// Default core block content name.
    pub default_core: Option<String>,
    /// Default starting sector.
    pub start_sector: u32,
    /// Base-generation seed (`-1` = random from id).
    pub sector_seed: i32,
    /// Blocks replaced on sector capture.
    pub sector_capture_replacements: Vec<(String, String)>,
    /// Content unlocked on landing (block names).
    pub unlocked_on_land: Vec<String>,
    /// Tech tree for this planet.
    pub tech_tree: Option<TreeId>,
    /// Launch-schematic allowance.
    pub allow_launch_schematics: bool,
    /// Launch-loadout allowance.
    pub allow_launch_loadout: bool,
    /// Sector invasion simulation.
    pub allow_sector_invasion: bool,
    /// Legacy launch pads.
    pub allow_legacy_launch_pads: bool,
    /// Waves on sector loss.
    pub allow_waves: bool,
    /// Numbered-sector launches.
    pub allow_launch_to_numbered: bool,
    /// Self-sector launch.
    pub allow_self_sector_launch: bool,
    /// Customizable RTS AI rule.
    pub show_rts_ai_rule: bool,
    /// Clear sector saves on loss.
    pub clear_sector_on_lose: bool,
    /// Enemy cores replaced by spawnpoints.
    pub enemy_core_spawn_replace: bool,
    /// Loads `planets/<name>.json`.
    pub load_planet_data: bool,
    /// Enemy rebuild speed multiplier.
    pub enemy_build_speed_multiplier: f32,
    /// Enemy factory activation delay.
    pub enemy_factory_activation_delay: f32,
    /// Core capacity multiplier when launching.
    pub launch_capacity_multiplier: f32,
    /// Atmosphere inner radius adjustment.
    pub atmosphere_rad_in: f32,
    /// Atmosphere outer radius adjustment.
    pub atmosphere_rad_out: f32,
    /// Tidally locked.
    pub tidal_lock: bool,
    /// Orbit spacing (defined per parent).
    pub orbit_spacing: f32,
    /// Total radius including children.
    pub total_radius: f32,
    /// Light source interpolation.
    pub light_src_to: f32,
    /// Light destination interpolation.
    pub light_dst_from: f32,
    /// Camera radius.
    pub cam_radius: f32,
    /// Minimum camera zoom.
    pub min_zoom: f32,
    /// Frustum clip radius.
    pub clip_radius: f32,
    /// Whether to draw the orbit.
    pub draw_orbit: bool,
    /// Planet-list icon name.
    pub icon: String,
    /// Asteroid generator spec (asteroids only).
    pub asteroid: Option<AsteroidSpec>,
}

impl PlanetDef {
    /// Creates a planet with upstream defaults (`Planet(String, Planet, float)`).
    pub fn new(
        name: &str,
        parent: Option<PlanetId>,
        radius: f32,
        bundle: &dyn BundleView,
        store: &dyn UnlockStore,
    ) -> Self {
        Self {
            id: PlanetId::new(0),
            name: name.to_owned(),
            minfo: ModContentInfo::default(),
            removed: false,
            unlock: UnlockFields::new(ContentType::Planet, name, bundle, store),
            parent,
            radius,
            sector_tiles: 0,
            sector_count: 0,
            sectors: Vec::new(),
            generator: GeneratorKind::None,
            mesh: MeshKind::ShaderSphere,
            cloud_mesh: None,
            bloom: false,
            accessible: true,
            visible: true,
            has_atmosphere: true,
            update_lighting: true,
            land_cloud_color: Rgba::new(1.0, 1.0, 1.0, 0.5),
            light_color: Rgba::WHITE,
            atmosphere_color: Rgba::new(0.3, 0.7, 1.0, 1.0),
            icon_color: Rgba::WHITE,
            default_env: vec![
                EnvFlag::Terrestrial,
                EnvFlag::Spores,
                EnvFlag::GroundOil,
                EnvFlag::GroundWater,
                EnvFlag::Oxygen,
            ],
            default_attributes: Vec::new(),
            rule_setter: RuleSetterKind::None,
            campaign_rule_defaults: CampaignRuleDefaults::default(),
            default_core: Some(String::from("core-shard")),
            start_sector: 0,
            sector_seed: -1,
            sector_capture_replacements: Vec::new(),
            unlocked_on_land: Vec::new(),
            tech_tree: None,
            allow_launch_schematics: false,
            allow_launch_loadout: false,
            allow_sector_invasion: false,
            allow_legacy_launch_pads: false,
            allow_waves: false,
            allow_launch_to_numbered: true,
            allow_self_sector_launch: false,
            show_rts_ai_rule: false,
            clear_sector_on_lose: false,
            enemy_core_spawn_replace: false,
            load_planet_data: false,
            enemy_build_speed_multiplier: 1.0,
            enemy_factory_activation_delay: 0.0,
            launch_capacity_multiplier: 0.25,
            atmosphere_rad_in: 0.0,
            atmosphere_rad_out: 0.3,
            tidal_lock: false,
            orbit_spacing: 12.0,
            total_radius: radius,
            light_src_to: 0.8,
            light_dst_from: 0.2,
            cam_radius: 0.0,
            min_zoom: 0.5,
            clip_radius: -1.0,
            draw_orbit: true,
            icon: String::from("planet"),
            asteroid: None,
        }
    }

    /// Builds the sector grid (`Planet(String, Planet, float, int)`).
    pub fn with_sector_grid(mut self, sector_tiles: u8) -> Self {
        self.sector_tiles = sector_tiles;
        self.sector_count = sector_count_for(sector_tiles);
        self.sectors = (0..self.sector_count)
            .map(|index| Sector::new(index as u16))
            .collect();
        self
    }

    /// Sets a sector preset (`Planet.preset(index, preset)`).
    pub fn set_preset(&mut self, index: usize, preset: SectorId) {
        if let Some(sector) = self.sectors.get_mut(index) {
            sector.preset = Some(preset);
        }
    }

    /// Adds an environment attribute.
    pub fn set_attribute(&mut self, attribute: Attribute, value: f32) {
        if let Some(entry) = self
            .default_attributes
            .iter_mut()
            .find(|(existing, _)| *existing == attribute)
        {
            entry.1 = value;
        } else {
            self.default_attributes.push((attribute, value));
        }
    }

    /// `Planet.getStartSector`.
    pub fn start_sector(&self) -> Option<&Sector> {
        self.sectors.get(self.start_sector as usize)
    }
}

impl Content for PlanetDef {
    const TYPE: ContentType = ContentType::Planet;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = PlanetId::new(id);
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
        "Planet"
    }

    fn content_name(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn unlock_fields(&self) -> Option<&UnlockFields> {
        Some(&self.unlock)
    }

    fn post_init(&mut self) -> Result<(), ContentError> {
        self.unlock.post_init();
        Ok(())
    }
}

impl Mappable for PlanetDef {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Unlockable for PlanetDef {
    fn unlock(&self) -> &UnlockFields {
        &self.unlock
    }

    fn unlock_mut(&mut self) -> &mut UnlockFields {
        &mut self.unlock
    }
}

/// `Planets.makeAsteroid` helper.
#[allow(clippy::too_many_arguments)]
fn make_asteroid(
    registry: &mut ContentRegistry,
    name: &str,
    parent: PlanetId,
    base: &str,
    tint: &str,
    seed: i32,
    tint_thresh: f32,
    pieces: i32,
    scale: f32,
    params: &[(&'static str, f32)],
    bundle: &dyn BundleView,
    store: &dyn UnlockStore,
) -> Result<PlanetId, ContentError> {
    let mut planet = PlanetDef::new(name, Some(parent), 0.12, bundle, store);
    planet.has_atmosphere = false;
    planet.update_lighting = false;
    planet.sector_count = 1;
    planet.sectors = vec![Sector::new(0)];
    planet.cam_radius = 0.68 * scale;
    planet.min_zoom = 0.6;
    planet.draw_orbit = false;
    planet.accessible = false;
    planet.clip_radius = 2.0;
    planet.default_env = vec![EnvFlag::Space];
    planet.icon = String::from("commandRally");
    planet.generator = GeneratorKind::Asteroid;
    planet.mesh = MeshKind::Asteroid;
    planet.asteroid = Some(AsteroidSpec {
        base: base.to_owned(),
        tint: tint.to_owned(),
        seed,
        tint_thresh,
        pieces,
        scale,
        params: params.to_vec(),
    });
    registry.add_planet(planet)
}

/// Loads all 7 vanilla planets in `Planets.load()` order.
pub fn load(
    registry: &mut ContentRegistry,
    bundle: &dyn BundleView,
    store: &dyn UnlockStore,
) -> Result<(), ContentError> {
    let sun = registry.add_planet({
        let mut planet = PlanetDef::new("sun", None, 4.0, bundle, store);
        planet.bloom = true;
        planet.accessible = false;
        planet.mesh = MeshKind::Sun;
        planet
    })?;

    let erekir = registry.add_planet({
        let mut planet =
            PlanetDef::new("erekir", Some(sun), 1.0, bundle, store).with_sector_grid(2);
        planet.generator = GeneratorKind::Erekir;
        planet.mesh = MeshKind::Hex { detail: 5 };
        planet.cloud_mesh = Some(CloudMeshKind::MultiHex);
        planet.unlock.always_unlocked = true;
        planet.land_cloud_color = Rgba::from_hex("ed6542").unwrap_or(Rgba::WHITE);
        planet.atmosphere_color = Rgba::from_hex("f07218").unwrap_or(Rgba::WHITE);
        planet.default_env = vec![EnvFlag::Scorching, EnvFlag::Terrestrial];
        planet.start_sector = 10;
        planet.atmosphere_rad_in = 0.02;
        planet.atmosphere_rad_out = 0.3;
        planet.tidal_lock = true;
        planet.orbit_spacing = 2.0;
        planet.total_radius += 2.6;
        planet.light_src_to = 0.5;
        planet.light_dst_from = 0.2;
        planet.clear_sector_on_lose = true;
        planet.default_core = Some(String::from("core-bastion"));
        planet.icon_color = Rgba::from_hex("ff9266").unwrap_or(Rgba::WHITE);
        planet.enemy_build_speed_multiplier = 0.4;
        planet.allow_launch_to_numbered = false;
        planet.update_lighting = false;
        planet.set_attribute(Attribute::Heat, 0.8);
        planet.rule_setter = RuleSetterKind::Erekir;
        planet.campaign_rule_defaults = CampaignRuleDefaults {
            fog: true,
            hide_spawns: false,
            rts_ai: true,
        };
        planet.unlocked_on_land = vec![String::from("core-bastion")];
        planet
    })?;

    make_asteroid(
        registry,
        "gier",
        erekir,
        "ferric-stone-wall",
        "carbon-wall",
        -5,
        0.4,
        7,
        1.0,
        &[
            ("min", 25.0),
            ("max", 35.0),
            ("carbonChance", 0.6),
            ("iceChance", 0.0),
            ("berylChance", 0.1),
        ],
        bundle,
        store,
    )?;

    make_asteroid(
        registry,
        "notva",
        sun,
        "ferric-stone-wall",
        "beryllic-stone-wall",
        -4,
        0.55,
        9,
        1.3,
        &[
            ("berylChance", 0.8),
            ("iceChance", 0.0),
            ("carbonChance", 0.01),
            ("max", 2.0),
        ],
        bundle,
        store,
    )?;

    registry.add_planet({
        let mut planet =
            PlanetDef::new("tantros", Some(sun), 1.0, bundle, store).with_sector_grid(2);
        planet.generator = GeneratorKind::Tantros;
        planet.mesh = MeshKind::Hex { detail: 4 };
        planet.accessible = false;
        planet.visible = false;
        planet.atmosphere_color = Rgba::from_hex("3db899").unwrap_or(Rgba::WHITE);
        planet.icon_color = Rgba::from_hex("597be3").unwrap_or(Rgba::WHITE);
        planet.start_sector = 10;
        planet.atmosphere_rad_in = -0.01;
        planet.atmosphere_rad_out = 0.3;
        planet.default_env = vec![EnvFlag::Underwater, EnvFlag::Terrestrial];
        planet.rule_setter = RuleSetterKind::Tantros;
        planet
    })?;

    let serpulo = registry.add_planet({
        let mut planet =
            PlanetDef::new("serpulo", Some(sun), 1.0, bundle, store).with_sector_grid(3);
        planet.load_planet_data = true;
        planet.generator = GeneratorKind::Serpulo;
        planet.mesh = MeshKind::Hex { detail: 6 };
        planet.cloud_mesh = Some(CloudMeshKind::MultiHex);
        planet.enemy_factory_activation_delay = 60.0 * 60.0 * 2.0;
        planet.launch_capacity_multiplier = 0.5;
        planet.sector_seed = 2;
        planet.allow_waves = true;
        planet.allow_legacy_launch_pads = true;
        planet.allow_sector_invasion = true;
        planet.allow_launch_schematics = true;
        planet.enemy_core_spawn_replace = true;
        planet.allow_launch_loadout = true;
        planet.rule_setter = RuleSetterKind::Serpulo;
        planet.show_rts_ai_rule = true;
        planet.icon_color = Rgba::from_hex("7d4dff").unwrap_or(Rgba::WHITE);
        planet.atmosphere_color = Rgba::from_hex("3c1b8f").unwrap_or(Rgba::WHITE);
        planet.atmosphere_rad_in = 0.02;
        planet.atmosphere_rad_out = 0.3;
        planet.start_sector = 170;
        planet.unlock.always_unlocked = true;
        planet.allow_self_sector_launch = true;
        let spore = Rgba::from_hex("7457ce").unwrap_or(Rgba::WHITE);
        planet.land_cloud_color = spore.with_alpha(0.5);
        planet.sector_capture_replacements = vec![
            (
                String::from("metal-tiles-12"),
                String::from("metal-tiles-11"),
            ),
            (
                String::from("metal-tiles-6"),
                String::from("metal-tiles-10"),
            ),
        ];
        planet
    })?;
    debug_assert_eq!(serpulo, PlanetId::new(5));

    make_asteroid(
        registry,
        "verilus",
        sun,
        "stone-wall",
        "ice-wall",
        -1,
        0.5,
        12,
        2.0,
        &[
            ("berylChance", 0.0),
            ("iceChance", 0.6),
            ("carbonChance", 0.1),
            ("ferricChance", 0.0),
        ],
        bundle,
        store,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::test_registry;
    use super::*;

    #[test]
    fn load_order_and_grids() {
        let registry = test_registry();
        assert_eq!(registry.planets().len(), 7, "planet count");
        let names: Vec<&str> = registry
            .planets()
            .iter()
            .map(|planet| planet.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "sun", "erekir", "gier", "notva", "tantros", "serpulo", "verilus"
            ]
        );
        let planet_id = |name: &str| registry.planet_by_name(name).unwrap().id;
        let erekir = registry.planet(planet_id("erekir")).unwrap();
        assert_eq!(erekir.parent, Some(planet_id("sun")));
        assert_eq!(erekir.sector_count, 92);
        assert_eq!(erekir.sector_count, sector_count_for(2));
        assert_eq!(erekir.start_sector, 10);
        assert_eq!(erekir.default_core.as_deref(), Some("core-bastion"));
        assert_eq!(erekir.attr(Attribute::Heat), 0.8);

        let serpulo = registry.planet(planet_id("serpulo")).unwrap();
        assert_eq!(serpulo.sector_count, 272);
        assert_eq!(serpulo.start_sector, 170);
        assert_eq!(serpulo.sector_seed, 2);
        assert_eq!(serpulo.sector_capture_replacements.len(), 2);

        let gier = registry.planet(planet_id("gier")).unwrap();
        assert_eq!(gier.parent, Some(planet_id("erekir")));
        assert_eq!(gier.sector_count, 1);
        assert_eq!(gier.generator, GeneratorKind::Asteroid);
        assert!(gier.asteroid.is_some());

        let sun = registry.planet(planet_id("sun")).unwrap();
        assert_eq!(sun.parent, None);
        assert_eq!(sun.mesh, MeshKind::Sun);
        assert!(!sun.accessible);
    }

    impl PlanetDef {
        fn attr(&self, attribute: Attribute) -> f32 {
            self.default_attributes
                .iter()
                .find(|(existing, _)| *existing == attribute)
                .map(|(_, value)| *value)
                .unwrap_or(0.0)
        }
    }
}
