// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Rules`/`GameStats`/`MapLocales`/`SectorInfo` JSON shapes (plan 04 §3.6,
//! §6.7). Runtime behavior (`Rules.mode()`, team helpers, sector launch logic)
//! is plan 12's; this module is the persistence shape.
//!
//! Ported from `core/src/mindustry/game/{Rules,GameStats,SectorInfo}.java` and
//! `core/src/mindustry/type/MapLocales.java`. Field names stay exactly
//! upstream camelCase (OD9); every struct is `#[serde(default)]` so old and
//! new JSON both load (forward/backward tolerance). Content-valued fields are
//! stored as **names** and resolved with the upstream fallback table through
//! [`super::content_serde`] when applied (plan 12).

use indexmap::{IndexMap, IndexSet};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::content_serde::{ColorHex, JsonItemStack, MusicContainer, SectorKey};
use super::objectives::MapObjectives;
use crate::content::{Attribute, Rgba};
use crate::io::StringMap;

/// `Vars.defaultEnv` (`Env.terrestrial | Env.spores | Env.groundOil |
/// Env.groundWater | Env.oxygen`).
pub const DEFAULT_ENV: i32 = 233;

/// `Time.toMinutes` in ticks (`60 * 60`).
pub const TIME_TO_MINUTES: f32 = 60.0 * 60.0;

/// One team's rule overrides (`Rules.TeamRule`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TeamRule {
    /// Ships spawn from the core (TODO upstream).
    pub ai_core_spawn: bool,
    /// Core no-build protection applies.
    pub protect_cores: bool,
    /// `placeRangeCheck` applies.
    pub check_placement: bool,
    /// Blocks don't require power/resources.
    pub cheat: bool,
    /// Core always full of every item.
    pub fill_items: bool,
    /// Resources aren't consumed.
    pub infinite_resources: bool,
    /// EXPERIMENTAL pre-built base AI.
    pub prebuild_ai: bool,
    /// Random schematic builder AI.
    pub build_ai: bool,
    /// Builder AI tier `[0, 1]`.
    pub build_ai_tier: f32,
    /// RTS unit AI.
    pub rts_ai: bool,
    /// Minimum attack squad size.
    pub rts_min_squad: i32,
    /// Maximum attack squad size.
    pub rts_max_squad: i32,
    /// Minimum attack "advantage" weight.
    pub rts_min_weight: f32,
    /// Unit factory activation delay.
    pub unit_factory_activation_delay: f32,
    /// Unit build speed multiplier.
    pub unit_build_speed_multiplier: f32,
    /// Unit damage multiplier.
    pub unit_damage_multiplier: f32,
    /// Unit crash damage multiplier.
    pub unit_crash_damage_multiplier: f32,
    /// Unit mining speed multiplier.
    pub unit_mine_speed_multiplier: f32,
    /// Unit cost multiplier.
    pub unit_cost_multiplier: f32,
    /// Unit health multiplier.
    pub unit_health_multiplier: f32,
    /// Block health multiplier.
    pub block_health_multiplier: f32,
    /// Block damage multiplier.
    pub block_damage_multiplier: f32,
    /// Build speed multiplier.
    pub build_speed_multiplier: f32,
    /// Extra no-build radius.
    pub extra_core_build_radius: f32,
}

impl Default for TeamRule {
    fn default() -> Self {
        Self {
            ai_core_spawn: true,
            protect_cores: true,
            check_placement: true,
            cheat: false,
            fill_items: false,
            infinite_resources: false,
            prebuild_ai: false,
            build_ai: false,
            build_ai_tier: 1.0,
            rts_ai: false,
            rts_min_squad: 4,
            rts_max_squad: 50,
            rts_min_weight: 1.2,
            unit_factory_activation_delay: 0.0,
            unit_build_speed_multiplier: 1.0,
            unit_damage_multiplier: 1.0,
            unit_crash_damage_multiplier: 1.0,
            unit_mine_speed_multiplier: 1.0,
            unit_cost_multiplier: 1.0,
            unit_health_multiplier: 1.0,
            block_health_multiplier: 1.0,
            block_damage_multiplier: 1.0,
            build_speed_multiplier: 1.0,
            extra_core_build_radius: 0.0,
        }
    }
}

/// Per-team rule overrides keyed by team id (`Rules.TeamRules`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TeamRules(pub IndexMap<u8, TeamRule>);

impl TeamRules {
    /// Empty overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Entry for `team_id`, if present.
    pub fn get(&self, team_id: u8) -> Option<&TeamRule> {
        self.0.get(&team_id)
    }

    /// Entry for `team_id`, inserting the upstream defaults on first use.
    pub fn get_or_default(&mut self, team_id: u8) -> &TeamRule {
        self.0.entry(team_id).or_default()
    }
}

/// Environment attributes (`mindustry.world.blocks.Attributes`): a name-keyed
/// float map that serializes only non-zero values.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Attributes(pub IndexMap<String, f32>);

impl Attributes {
    /// Value for a known attribute (0 when absent).
    pub fn get(&self, attribute: Attribute) -> f32 {
        self.get_by_name(attribute.name())
    }

    /// Value for an attribute name (0 when absent — mod attribute names too).
    pub fn get_by_name(&self, name: &str) -> f32 {
        self.0.get(name).copied().unwrap_or(0.0)
    }

    /// Sets a known attribute (`Attributes.set`); zero removes the entry like
    /// the upstream zero-skipping writer.
    pub fn set(&mut self, attribute: Attribute, value: f32) {
        self.set_by_name(attribute.name(), value);
    }

    /// Sets an attribute by name.
    pub fn set_by_name(&mut self, name: &str, value: f32) {
        if value == 0.0 {
            self.0.shift_remove(name);
        } else {
            self.0.insert(name.to_owned(), value);
        }
    }

    /// Whether no attribute is set.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for Attributes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        for (name, value) in &self.0 {
            if *value != 0.0 {
                map.serialize_entry(name, value)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Attributes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(IndexMap::<String, f32>::deserialize(deserializer)?))
    }
}

/// `Vec3` wire shape.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Vec3 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
    /// Z component.
    pub z: f32,
}

impl Default for Vec3 {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }
}

/// Planet render parameters (`mindustry.graphics.g3d.PlanetParams`); only the
/// persisted (non-transient) fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlanetParams {
    /// Camera direction relative to the planet.
    pub cam_pos: Vec3,
    /// Previous planet camera position (`None` = absent).
    pub other_cam_pos: Option<Vec3>,
    /// Interpolation value for `other_cam_pos`.
    pub other_cam_alpha: f32,
    /// Camera up vector.
    pub cam_up: Vec3,
    /// Camera direction vector.
    pub cam_dir: Vec3,
    /// Planet being looked at.
    pub planet: String,
    /// Zoom relative to the planet.
    pub zoom: f32,
    /// Orbit/grid UI alpha.
    pub ui_alpha: f32,
    /// Draw orbit and sector grid.
    pub draw_ui: bool,
    /// Draw the space skybox.
    pub draw_skybox: bool,
}

impl Default for PlanetParams {
    fn default() -> Self {
        Self {
            cam_pos: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 4.0,
            },
            other_cam_pos: None,
            other_cam_alpha: 0.0,
            cam_up: Vec3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
            cam_dir: Vec3 {
                x: 0.0,
                y: 0.0,
                z: -1.0,
            },
            planet: "serpulo".to_owned(),
            zoom: 1.0,
            ui_alpha: 1.0,
            draw_ui: false,
            draw_skybox: true,
        }
    }
}

/// `SpawnGroup.never` (`Integer.MAX_VALUE`; `i32::MAX as f32` in the float
/// fields, matching the Java int-to-float conversion).
pub const NEVER: i32 = i32::MAX;
/// `SpawnGroup.never` as stored in `unitScaling`.
pub const NEVER_F32: f32 = i32::MAX as f32;

/// One wave-spawn group (`mindustry.game.SpawnGroup`).
///
/// Upstream's custom writer omits default-valued fields; the same shape is
/// kept with `skip_serializing_if`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SpawnGroup {
    /// Unit type name (`"dagger"` default; legacy names resolved on apply).
    #[serde(rename = "type")]
    pub type_: String,
    /// First wave this group spawns (0).
    #[serde(skip_serializing_if = "is_zero")]
    pub begin: i32,
    /// Last wave (`i32::MAX` = never ends).
    #[serde(skip_serializing_if = "is_never")]
    pub end: i32,
    /// Wave spacing (`1` = every wave).
    #[serde(skip_serializing_if = "is_one")]
    pub spacing: i32,
    /// Maximum units spawned.
    #[serde(skip_serializing_if = "is_40")]
    pub max: i32,
    /// Waves per +1 amount (upstream JSON key `scaling`).
    #[serde(rename = "scaling", skip_serializing_if = "is_never_f32")]
    pub unit_scaling: f32,
    /// Initial shields.
    #[serde(skip_serializing_if = "is_zero_f32")]
    pub shields: f32,
    /// Shield growth per wave.
    #[serde(skip_serializing_if = "is_zero_f32")]
    pub shield_scaling: f32,
    /// Initial amount (upstream JSON key `amount`).
    #[serde(rename = "amount", skip_serializing_if = "is_one")]
    pub unit_amount: i32,
    /// Status effect applied to spawned units.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    /// Pinned spawn point (`-1` = any).
    #[serde(skip_serializing_if = "is_minus_one")]
    pub spawn: i32,
    /// Payload unit names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payloads: Option<Vec<String>>,
    /// Items the unit spawns with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<JsonItemStack>,
    /// Team override (`None` = default wave team).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<u8>,
}

impl Default for SpawnGroup {
    fn default() -> Self {
        Self {
            type_: "dagger".to_owned(),
            begin: 0,
            end: NEVER,
            spacing: 1,
            max: 40,
            unit_scaling: NEVER_F32,
            shields: 0.0,
            shield_scaling: 0.0,
            unit_amount: 1,
            effect: None,
            spawn: -1,
            payloads: None,
            items: None,
            team: None,
        }
    }
}

fn is_zero(value: &i32) -> bool {
    *value == 0
}
fn is_one(value: &i32) -> bool {
    *value == 1
}
fn is_40(value: &i32) -> bool {
    *value == 40
}
fn is_minus_one(value: &i32) -> bool {
    *value == -1
}
fn is_never(value: &i32) -> bool {
    *value == NEVER
}
fn is_zero_f32(value: &f32) -> bool {
    *value == 0.0
}
fn is_never_f32(value: &f32) -> bool {
    *value == NEVER_F32
}

/// One weather schedule entry (`mindustry.type.Weather.WeatherEntry`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WeatherEntry {
    /// Weather content name.
    pub weather: String,
    /// Minimum frequency between events.
    pub min_frequency: f32,
    /// Maximum frequency between events.
    pub max_frequency: f32,
    /// Minimum event duration.
    pub min_duration: f32,
    /// Maximum event duration.
    pub max_duration: f32,
    /// Cooldown before the next event (state, but persisted upstream).
    pub cooldown: f32,
    /// Weather intensity.
    pub intensity: f32,
    /// Whether the weather is always active.
    pub always: bool,
}

impl Default for WeatherEntry {
    fn default() -> Self {
        Self {
            weather: "rain".to_owned(),
            min_frequency: 0.0,
            max_frequency: 0.0,
            min_duration: 0.0,
            max_duration: 0.0,
            cooldown: 0.0,
            intensity: 1.0,
            always: false,
        }
    }
}

/// Match configuration (`mindustry.game.Rules`).
///
/// Every field carries the upstream default and is namespaced exactly like
/// the Java field (OD9). Runtime helpers (`mode()`, team multipliers) are
/// plan 12's; `copy()` is a plain `Clone` (deviation 8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Rules {
    /// Allow editing rules in-game.
    pub allow_edit_rules: bool,
    /// Sandbox infinite resources/range/speed.
    pub infinite_resources: bool,
    /// Build/deconstruct cores anywhere + team UI.
    pub core_build_and_config: bool,
    /// Per-team rules.
    pub teams: TeamRules,
    /// Waves come automatically.
    pub wave_timer: bool,
    /// Waves can be sent manually.
    pub wave_sending: bool,
    /// Waves are enabled at all.
    pub waves: bool,
    /// Air units spawn at spawn points instead of the map edge.
    pub air_use_spawns: bool,
    /// Attack maps spawn units at enemy cores.
    pub waves_spawn_at_cores: bool,
    /// PvP game mode.
    pub pvp: bool,
    /// PvP auto-pause waiting for players.
    pub pvp_auto_pause: bool,
    /// Pause disabled in singleplayer.
    pub pause_disabled: bool,
    /// Pause the wave timer until all enemies are dead.
    pub wait_enemies: bool,
    /// Attack mode.
    pub attack_mode: bool,
    /// Editor mode.
    pub editor: bool,
    /// Blocks can be repaired by clicking.
    pub derelict_repair: bool,
    /// Gameover can happen.
    pub can_game_over: bool,
    /// Cores change teams when destroyed.
    pub core_capture: bool,
    /// Reactors explode.
    pub reactor_explosions: bool,
    /// Manual unit control.
    pub possession_allowed: bool,
    /// Schematics allowed.
    pub schematics_allowed: bool,
    /// Friendly explosions damage blocks.
    pub damage_explosions: bool,
    /// Fire/neoplasm spread enabled.
    pub fire: bool,
    /// Random wave AI targeting.
    pub random_wave_ai: bool,
    /// EXPERIMENTAL: blocks update in units and share power.
    pub unit_payload_update: bool,
    /// Destroy unit payloads on death.
    pub unit_payloads_explode: bool,
    /// Cores add to unit cap.
    pub unit_cap_variable: bool,
    /// Hide spawn points.
    pub hide_spawns: bool,
    /// Solar panel output multiplier.
    pub solar_multiplier: f32,
    /// Unit factory build speed multiplier.
    pub unit_build_speed_multiplier: f32,
    /// Unit cost multiplier.
    pub unit_cost_multiplier: f32,
    /// Unit damage multiplier.
    pub unit_damage_multiplier: f32,
    /// Unit health multiplier.
    pub unit_health_multiplier: f32,
    /// Unit crash damage multiplier.
    pub unit_crash_damage_multiplier: f32,
    /// Unit mining speed multiplier.
    pub unit_mine_speed_multiplier: f32,
    /// Global unit factory activation delay.
    pub unit_factory_activation_delay: f32,
    /// Dead blocks leave rebuildable ghosts.
    pub ghost_blocks: bool,
    /// Show pings from other teams.
    pub show_other_team_pings: bool,
    /// Logic may control units.
    pub logic_unit_control: bool,
    /// Logic units may build.
    pub logic_unit_build: bool,
    /// Logic units may deconstruct.
    pub logic_unit_deconstruct: bool,
    /// World processors may link to player structures.
    pub world_processor_player_link: bool,
    /// World processors can be edited/placed.
    pub allow_edit_world_processors: bool,
    /// World processors no longer update.
    pub disable_world_processors: bool,
    /// Block health multiplier.
    pub block_health_multiplier: f32,
    /// Block damage multiplier.
    pub block_damage_multiplier: f32,
    /// Building cost multiplier.
    pub build_cost_multiplier: f32,
    /// Building speed multiplier.
    pub build_speed_multiplier: f32,
    /// Deconstruction refund fraction.
    pub deconstruct_refund_multiplier: f32,
    /// Timer objective duration multiplier.
    pub objective_timer_multiplier: f32,
    /// Enemy core no-build radius.
    pub enemy_core_build_radius: f32,
    /// No-build zones from the closest core.
    pub polygon_core_protection: bool,
    /// Placement range check near enemy blocks.
    pub place_range_check: bool,
    /// Dead teams convert to derelict.
    pub cleanup_dead_teams: bool,
    /// Items can only be deposited in cores.
    pub only_deposit_core: bool,
    /// Unloaders may take from cores.
    pub allow_core_unloaders: bool,
    /// Item deposit cooldown in seconds.
    pub item_deposit_cooldown: f32,
    /// Enemy core death clears its radius.
    pub core_destroy_clear: bool,
    /// Hide banned blocks from the build menu.
    pub hide_banned_blocks: bool,
    /// Environmental blocks can be deconstructed.
    pub allow_environment_deconstruct: bool,
    /// Instant building (experimental).
    pub instant_build: bool,
    /// `banned_blocks` is a whitelist.
    pub block_whitelist: bool,
    /// `banned_units` is a whitelist.
    pub unit_whitelist: bool,
    /// Enemy wave drop-zone radius.
    pub drop_zone_radius: f32,
    /// Time between waves in ticks.
    pub wave_spacing: f32,
    /// Initial wave spacing (`<=0` = `wave_spacing * 2`).
    pub initial_wave_spacing: f32,
    /// Wave after which the player wins (`<=0` disables).
    pub win_wave: i32,
    /// Base unit cap.
    pub unit_cap: i32,
    /// Disable the unit cap.
    pub disable_unit_cap: bool,
    /// Environment drag multiplier.
    pub drag_multiplier: f32,
    /// Environment flags (`Env` bitmask).
    pub env: i32,
    /// Environment attributes.
    pub attributes: Attributes,
    /// Sector this save belongs to.
    pub sector: Option<SectorKey>,
    /// Ambient music override.
    pub ambient_music: Option<Vec<MusicContainer>>,
    /// Situational music override.
    pub dark_music: Option<Vec<MusicContainer>>,
    /// Always play ambient music.
    pub always_play_music: bool,
    /// Disable automatic music.
    pub disable_music: bool,
    /// Music volume multiplier.
    pub music_volume: f32,
    /// Wave spawn groups.
    pub spawns: Vec<SpawnGroup>,
    /// Starting core items.
    pub loadout: Vec<JsonItemStack>,
    /// Weather schedule.
    pub weather: Vec<WeatherEntry>,
    /// Placement limits per block name.
    pub block_limits: IndexMap<String, i32>,
    /// Unplaceable block names.
    pub banned_blocks: IndexSet<String>,
    /// Unbuildable unit names.
    pub banned_units: IndexSet<String>,
    /// Blocks revealed despite build visibility.
    pub revealed_blocks: IndexSet<String>,
    /// Researched content names (multiplayer campaign).
    pub researched: IndexSet<String>,
    /// In-map objective executor.
    pub objectives: MapObjectives,
    /// Flags set by objectives.
    pub objective_flags: IndexSet<String>,
    /// Fog of war enabled.
    pub fog: bool,
    /// Static (black) fog enabled.
    pub static_fog: bool,
    /// Static fog color.
    pub static_color: ColorHex,
    /// Dynamic fog color.
    pub dynamic_color: ColorHex,
    /// Ambient lighting enabled.
    pub lighting: bool,
    /// Ambient light color.
    pub ambient_light: ColorHex,
    /// Units emit light.
    pub unit_light: bool,
    /// Player team.
    pub default_team: u8,
    /// Enemy wave team.
    pub wave_team: u8,
    /// Landing cloud color.
    pub cloud_color: ColorHex,
    /// Custom mode name.
    pub mode_name: Option<String>,
    /// Mission string override.
    pub mission: Option<String>,
    /// Cores incinerate items when full.
    pub core_incinerates: bool,
    /// Borders fade to darkness.
    pub border_darkness: bool,
    /// Crop the play area to the limit rectangle.
    pub limit_map_area: bool,
    /// Limit rectangle x.
    pub limit_x: i32,
    /// Limit rectangle y.
    pub limit_y: i32,
    /// Limit rectangle width.
    pub limit_width: i32,
    /// Limit rectangle height.
    pub limit_height: i32,
    /// Disable blocks outside the area.
    pub disable_outside_area: bool,
    /// Arbitrary map/rule tags.
    pub tags: StringMap,
    /// Background render callback name.
    pub custom_background_callback: Option<String>,
    /// Background texture path.
    pub background_texture: Option<String>,
    /// Background pan speed.
    pub background_speed: f32,
    /// Background scale.
    pub background_scl: f32,
    /// Background UV offset x.
    pub background_offset_x: f32,
    /// Background UV offset y.
    pub background_offset_y: f32,
    /// Planet background parameters.
    pub planet_background: PlanetParams,
    /// Planet whose tech/attributes apply.
    pub planet: String,
    /// Allow the logic `data` instruction.
    pub allow_logic_data: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            allow_edit_rules: false,
            infinite_resources: false,
            core_build_and_config: false,
            teams: TeamRules::new(),
            wave_timer: true,
            wave_sending: true,
            waves: false,
            air_use_spawns: false,
            waves_spawn_at_cores: true,
            pvp: false,
            pvp_auto_pause: true,
            pause_disabled: false,
            wait_enemies: false,
            attack_mode: false,
            editor: false,
            derelict_repair: true,
            can_game_over: true,
            core_capture: false,
            reactor_explosions: true,
            possession_allowed: true,
            schematics_allowed: true,
            damage_explosions: true,
            fire: true,
            random_wave_ai: false,
            unit_payload_update: false,
            unit_payloads_explode: false,
            unit_cap_variable: true,
            hide_spawns: true,
            solar_multiplier: 1.0,
            unit_build_speed_multiplier: 1.0,
            unit_cost_multiplier: 1.0,
            unit_damage_multiplier: 1.0,
            unit_health_multiplier: 1.0,
            unit_crash_damage_multiplier: 1.0,
            unit_mine_speed_multiplier: 1.0,
            unit_factory_activation_delay: 0.0,
            ghost_blocks: true,
            show_other_team_pings: false,
            logic_unit_control: true,
            logic_unit_build: true,
            logic_unit_deconstruct: false,
            world_processor_player_link: true,
            allow_edit_world_processors: false,
            disable_world_processors: false,
            block_health_multiplier: 1.0,
            block_damage_multiplier: 1.0,
            build_cost_multiplier: 1.0,
            build_speed_multiplier: 1.0,
            deconstruct_refund_multiplier: 0.5,
            objective_timer_multiplier: 1.0,
            enemy_core_build_radius: 400.0,
            polygon_core_protection: false,
            place_range_check: false,
            cleanup_dead_teams: true,
            only_deposit_core: false,
            allow_core_unloaders: true,
            item_deposit_cooldown: 0.5,
            core_destroy_clear: false,
            hide_banned_blocks: false,
            allow_environment_deconstruct: false,
            instant_build: false,
            block_whitelist: false,
            unit_whitelist: false,
            drop_zone_radius: 300.0,
            wave_spacing: 2.0 * TIME_TO_MINUTES,
            initial_wave_spacing: 0.0,
            win_wave: 0,
            unit_cap: 0,
            disable_unit_cap: false,
            drag_multiplier: 1.0,
            env: DEFAULT_ENV,
            attributes: Attributes::default(),
            sector: None,
            ambient_music: None,
            dark_music: None,
            always_play_music: false,
            disable_music: false,
            music_volume: 1.0,
            spawns: Vec::new(),
            loadout: vec![JsonItemStack {
                item: Some("copper".to_owned()),
                amount: 100,
            }],
            weather: Vec::new(),
            block_limits: IndexMap::new(),
            banned_blocks: IndexSet::new(),
            banned_units: IndexSet::new(),
            revealed_blocks: IndexSet::new(),
            researched: IndexSet::new(),
            objectives: MapObjectives::new(),
            objective_flags: IndexSet::new(),
            fog: false,
            static_fog: true,
            static_color: ColorHex(Rgba::new(0.0, 0.0, 0.0, 1.0)),
            dynamic_color: ColorHex(Rgba::new(0.0, 0.0, 0.0, 0.5)),
            lighting: false,
            ambient_light: ColorHex(Rgba::new(0.01, 0.01, 0.04, 0.99)),
            unit_light: true,
            default_team: 1,
            wave_team: 2,
            cloud_color: ColorHex(Rgba::CLEAR),
            mode_name: None,
            mission: None,
            core_incinerates: true,
            border_darkness: true,
            limit_map_area: false,
            limit_x: 0,
            limit_y: 0,
            limit_width: 1,
            limit_height: 1,
            disable_outside_area: true,
            tags: StringMap::new(),
            custom_background_callback: None,
            background_texture: None,
            background_speed: 27000.0,
            background_scl: 1.0,
            background_offset_x: 0.1,
            background_offset_y: 0.1,
            planet_background: PlanetParams::default(),
            planet: "serpulo".to_owned(),
            allow_logic_data: false,
        }
    }
}

/// Per-game stats (`mindustry.game.GameStats`); content-keyed maps use names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GameStats {
    /// Enemy units destroyed.
    pub enemy_units_destroyed: i32,
    /// Waves survived.
    pub waves_lasted: i32,
    /// Friendly buildings fully built.
    pub buildings_built: i32,
    /// Friendly buildings fully deconstructed.
    pub buildings_deconstructed: i32,
    /// Friendly buildings destroyed.
    pub buildings_destroyed: i32,
    /// Units created by any means.
    pub units_created: i32,
    /// Blocks placed by block name.
    pub placed_block_count: IndexMap<String, i32>,
    /// Enemy blocks destroyed by block name.
    pub destroyed_block_count: IndexMap<String, i32>,
    /// Items entering the core by item name.
    pub core_item_count: IndexMap<String, i32>,
}

impl Default for GameStats {
    fn default() -> Self {
        Self {
            enemy_units_destroyed: 0,
            waves_lasted: 0,
            buildings_built: 0,
            buildings_deconstructed: 0,
            buildings_destroyed: 0,
            units_created: 0,
            placed_block_count: IndexMap::new(),
            destroyed_block_count: IndexMap::new(),
            core_item_count: IndexMap::new(),
        }
    }
}

impl GameStats {
    /// `GameStats.getPlaced`.
    pub fn get_placed(&self, block: &str) -> i32 {
        self.placed_block_count.get(block).copied().unwrap_or(0)
    }

    /// `GameStats.getDestroyed`.
    pub fn get_destroyed(&self, block: &str) -> i32 {
        self.destroyed_block_count.get(block).copied().unwrap_or(0)
    }
}

/// Map-specific locale bundles (`mindustry.type.MapLocales`):
/// `{locale: {key: value}}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MapLocales(pub IndexMap<String, StringMap>);

impl MapLocales {
    /// Empty bundle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `locale` has `key`.
    pub fn contains(&self, locale: &str, key: &str) -> bool {
        self.0.get(locale).is_some_and(|map| map.contains_key(key))
    }

    /// Value lookup with the upstream `en` fallback + `???key???` marker.
    pub fn get_property(&self, locale: &str, key: &str) -> String {
        if let Some(value) = self.0.get(locale).and_then(|map| map.get(key)) {
            return value.clone();
        }
        if let Some(value) = self.0.get("en").and_then(|map| map.get(key)) {
            return value.clone();
        }
        format!("???{key}???")
    }

    /// `MapLocales.getFormatted`: replaces leading `@` placeholders in order.
    pub fn get_formatted(&self, locale: &str, key: &str, args: &[&str]) -> String {
        let mut result = self.get_property(locale, key);
        for arg in args {
            match result.find('@') {
                Some(index) => result.replace_range(index..index + 1, arg),
                None => break,
            }
        }
        result
    }
}

/// One averaged stat (`SectorInfo.ExportStat`); only `mean` is persisted.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ExportStat {
    /// Mean items per refresh period.
    pub mean: f32,
}

/// Per-sector persistent state (`mindustry.game.SectorInfo`), persisted fields
/// only (transient windows/caches excluded like Arc's serializer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SectorInfo {
    /// Core input statistics by item name.
    pub production: IndexMap<String, ExportStat>,
    /// Raw production statistics by item name.
    pub raw_production: IndexMap<String, ExportStat>,
    /// Export statistics by item name.
    pub export: IndexMap<String, ExportStat>,
    /// Import statistics by item name.
    pub imports: IndexMap<String, ExportStat>,
    /// Items stored in all cores by item name.
    pub items: IndexMap<String, i32>,
    /// Best available core block name.
    pub best_core_type: String,
    /// Max storage capacity.
    pub storage_capacity: i32,
    /// Whether a core is present.
    pub has_core: bool,
    /// Last sector preset name.
    pub last_preset_name: Option<String>,
    /// Last map width.
    pub last_width: i32,
    /// Last map height.
    pub last_height: i32,
    /// Whether the sector was ever fully captured.
    pub was_captured: bool,
    /// Sector launched from.
    pub origin: Option<SectorKey>,
    /// Launch destination.
    pub destination: Option<SectorKey>,
    /// Known resources (unlockable content names).
    pub resources: Vec<String>,
    /// Whether waves are enabled.
    pub waves: bool,
    /// Whether attack mode is enabled.
    pub attack: bool,
    /// Whether the sector has enemy spawns.
    pub has_spawns: bool,
    /// Landing attempts with a fresh core.
    pub attempts: i32,
    /// Current wave.
    pub wave: i32,
    /// Win wave.
    pub win_wave: i32,
    /// Time between waves.
    pub wave_spacing: f32,
    /// Packed core spawn position.
    pub spawn_position: i32,
    /// Minutes this sector has been captured.
    pub minutes_captured: f32,
    /// Light coverage in radius units.
    pub light_coverage: f32,
    /// Display name.
    pub name: Option<String>,
    /// Displayed icon region.
    pub icon: Option<String>,
    /// Displayed icon as content name.
    pub content_icon: Option<String>,
    /// Generated-wave version (`-1` = stale).
    pub wave_version: i32,
    /// Whether the sector was shown to the player.
    pub shown: bool,
    /// Import cooldown timers by item name.
    pub import_cooldown_timers: IndexMap<String, f32>,
}

impl Default for SectorInfo {
    fn default() -> Self {
        Self {
            production: IndexMap::new(),
            raw_production: IndexMap::new(),
            export: IndexMap::new(),
            imports: IndexMap::new(),
            items: IndexMap::new(),
            best_core_type: "core-shard".to_owned(),
            storage_capacity: 0,
            has_core: true,
            last_preset_name: None,
            last_width: 0,
            last_height: 0,
            was_captured: false,
            origin: None,
            destination: None,
            resources: Vec::new(),
            waves: true,
            attack: false,
            has_spawns: true,
            attempts: 0,
            wave: 1,
            win_wave: -1,
            wave_spacing: 2.0 * TIME_TO_MINUTES,
            spawn_position: 0,
            minutes_captured: 0.0,
            light_coverage: 0.0,
            name: None,
            icon: None,
            content_icon: None,
            wave_version: -1,
            shown: false,
            import_cooldown_timers: IndexMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::json::MapObjective;

    fn golden_rules() -> Rules {
        let mut rules = Rules {
            attack_mode: true,
            build_speed_multiplier: 99.1,
            ..Rules::default()
        };
        rules.tags.insert("blah".to_owned(), "bleh".to_owned());
        rules
    }

    /// Ported from `ApplicationTests.writeRules2` (JsonIO rules round-trip).
    #[test]
    fn rules_json_roundtrip() {
        let rules = golden_rules();
        let json = serde_json::to_string(&rules).unwrap();
        let res: Rules = serde_json::from_str(&json).unwrap();
        assert_eq!(res.build_speed_multiplier, rules.build_speed_multiplier);
        assert_eq!(res.attack_mode, rules.attack_mode);
        assert_eq!(res.tags.get("blah"), Some(&"bleh".to_owned()));
    }

    /// Rewrites the golden fixture from the current serializer. Maintainer
    /// tool only: run with `--ignored --nocapture` after an intentional shape
    /// change, then review the diff (the golden is source-derived — see plan
    /// 04 Changelog).
    #[test]
    #[ignore = "fixture generator; run manually with --ignored"]
    fn regenerate_rules_golden_fixture() {
        let actual = serde_json::to_string(&golden_rules()).unwrap();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/json/rules_golden.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, format!("{actual}\n")).unwrap();
        println!("wrote {path:?}");
    }

    /// Golden blob derived from `JsonIO.write` semantics (source-derived: no
    /// JVM is available in this environment — see plan 04 Changelog).
    #[test]
    fn rules_golden_blob_matches_fixture() {
        let rules = golden_rules();
        let actual = serde_json::to_string(&rules).unwrap();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/json/rules_golden.json");
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("missing golden fixture {path:?} ({error}); generated:\n{actual}")
        });
        assert_eq!(expected.trim_end(), actual);
    }

    /// Backward tolerance: a partial old-shaped blob loads with defaults.
    /// Forward tolerance: unknown fields are ignored.
    #[test]
    fn rules_forward_backward_tolerance() {
        let old = r#"{"attackMode":true,"buildSpeedMultiplier":2.5,"tags":{"blah":"bleh"}}"#;
        let rules: Rules = serde_json::from_str(old).unwrap();
        assert!(rules.attack_mode);
        assert_eq!(rules.build_speed_multiplier, 2.5);
        assert_eq!(rules.wave_spacing, 2.0 * TIME_TO_MINUTES);
        assert_eq!(rules.env, DEFAULT_ENV);
        assert_eq!(rules.default_team, 1);
        assert_eq!(rules.wave_team, 2);
        assert_eq!(rules.planet, "serpulo");
        assert_eq!(rules.block_limits.len(), 0);

        let future =
            r#"{"futureRule":true,"allowEditRules":true,"planet":"erekir","nestedFuture":{"x":1}}"#;
        let rules: Rules = serde_json::from_str(future).unwrap();
        assert!(rules.allow_edit_rules);
        assert_eq!(rules.planet, "erekir");
    }

    #[test]
    fn teams_and_attributes_roundtrip() {
        let mut rules = Rules::default();
        rules.teams.0.insert(
            2,
            TeamRule {
                build_speed_multiplier: 3.0,
                ..TeamRule::default()
            },
        );
        rules.attributes.set(Attribute::Heat, 0.8);
        let json = serde_json::to_string(&rules).unwrap();
        assert!(json.contains("\"teams\":{\"2\":{"));
        assert!(json.contains("\"attributes\":{\"heat\":0.8}"));
        let back: Rules = serde_json::from_str(&json).unwrap();
        assert_eq!(back.teams.get(2).unwrap().build_speed_multiplier, 3.0);
        assert_eq!(back.attributes.get(Attribute::Heat), 0.8);
    }

    #[test]
    fn spawn_group_omits_defaults_like_upstream() {
        let mut group = SpawnGroup::default();
        assert_eq!(
            serde_json::to_string(&group).unwrap(),
            "{\"type\":\"dagger\"}"
        );
        group.begin = 3;
        group.end = 10;
        group.unit_amount = 4;
        group.unit_scaling = 2.5;
        group.items = Some(JsonItemStack {
            item: Some("lead".to_owned()),
            amount: 30,
        });
        group.team = Some(2);
        let json = serde_json::to_string(&group).unwrap();
        assert_eq!(
            json,
            "{\"type\":\"dagger\",\"begin\":3,\"end\":10,\"scaling\":2.5,\"amount\":4,\"items\":{\"item\":\"lead\",\"amount\":30},\"team\":2}"
        );
        let back: SpawnGroup = serde_json::from_str(&json).unwrap();
        assert_eq!(back, group);
    }

    #[test]
    fn game_stats_roundtrip_with_content_names() {
        let mut stats = GameStats {
            enemy_units_destroyed: 7,
            ..GameStats::default()
        };
        stats.placed_block_count.insert("conveyor".to_owned(), 12);
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"placedBlockCount\":{\"conveyor\":12}"));
        let back: GameStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back.get_placed("conveyor"), 12);
        assert_eq!(back, stats);
    }

    #[test]
    fn map_locales_shape_and_fallbacks() {
        let locales: MapLocales = serde_json::from_str(
            r#"{"en":{"greeting":"hi @","only":"en"},"fr":{"greeting":"salut @"}}"#,
        )
        .unwrap();
        assert!(locales.contains("fr", "greeting"));
        assert_eq!(locales.get_property("en", "greeting"), "hi @");
        assert_eq!(locales.get_property("fr", "greeting"), "salut @");
        // Missing locale falls back to `en`.
        assert_eq!(locales.get_property("de", "only"), "en");
        assert_eq!(locales.get_property("de", "none"), "???none???");
        assert_eq!(
            locales.get_formatted("fr", "greeting", &["bob"]),
            "salut bob"
        );
        let json = serde_json::to_string(&locales).unwrap();
        assert_eq!(serde_json::from_str::<MapLocales>(&json).unwrap(), locales);
    }

    #[test]
    fn sector_info_roundtrip() {
        let mut info = SectorInfo {
            wave: 12,
            origin: Some(SectorKey("serpulo-10".to_owned())),
            ..SectorInfo::default()
        };
        info.resources.push("copper".to_owned());
        info.export
            .insert("silicon".to_owned(), ExportStat { mean: 3.5 });
        info.import_cooldown_timers.insert("lead".to_owned(), 0.25);
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"bestCoreType\":\"core-shard\""));
        assert!(json.contains("\"origin\":\"serpulo-10\""));
        assert!(json.contains("\"export\":{\"silicon\":{\"mean\":3.5}}"));
        let back: SectorInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, info);
        // Old JSON without the new fields still loads.
        let old: SectorInfo = serde_json::from_str("{}").unwrap();
        assert_eq!(old.wave, 1);
        assert_eq!(old.best_core_type, "core-shard");
    }

    #[test]
    fn rules_carry_objectives_with_parents() {
        let mut rules = Rules::default();
        rules
            .objectives
            .all
            .push(MapObjective::Flag(Default::default()));
        rules
            .objectives
            .all
            .push(MapObjective::DestroyUnits(Default::default()));
        rules.objectives.all[1].common_mut().parents = vec![0];
        let json = serde_json::to_string(&rules).unwrap();
        let back: Rules = serde_json::from_str(&json).unwrap();
        assert_eq!(back.objectives, rules.objectives);
        assert_eq!(back.objectives.get(1).unwrap().common().parents, vec![0]);
    }
}
