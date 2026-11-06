// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Server host control and the dedicated-server game loop (plan 22 §3.6/§3.11).
//!
//! Ported from `server/src/mindustry/server/ServerControl.java` (`host`/`stop`
//! `:512-568`, the `gameOverListener` `:116-133`, `play` `:1375-1407`, `mods`
//! `:641-651`, `fillitems` `:772-795`, `runwave` `:1143-1150`, `gameover`
//! `:1234-1243`) and `core/src/mindustry/core/Logic.java` (`play`/`runWave`).
//!
//! [`LocalHost`] owns a real `mind_core::sim::Sim`:
//!
//! - `host` loads a real world — a `.msav` map through the live-sim save engine
//!   (`SimIoHandler`), a generated campaign sector preset (`generate_sector` +
//!   `place_structures`), or the flat `synthetic` fixture — then installs the
//!   match [`PlaySession`] (`play_map` + `sync_session_with_sim`) and plays.
//! - `tick` is the fixed-step host loop: it advances the sim, runs the wave
//!   timer, honours `autoPause`, and restarts the next map `roundExtraTime`
//!   seconds after a game over.
//! - `run_wave` runs the real `Logic.runWave` (`run_wave_now`) instead of
//!   incrementing a counter; `fill_items` fills the team cores' item modules;
//!   `game_over` fires the upstream game-over path.
//!
//! The plan-21 networked match (`create_match`) is created by the console
//! `host` command through [`super::net_host::NetHost`]; this module stays
//! Godot-free and network-free.

use std::path::{Path, PathBuf};

use mind_core::command::Command;
use mind_core::content::{BlockId, BlockKind, ContentRegistry};
use mind_core::ecs::{BuildingComp, TeamId};
use mind_core::game::State;
use mind_core::game::gamemode::Gamemode;
use mind_core::game::planet::EmptyNeighborhood;
use mind_core::game::play::{PlaySession, play_map};
use mind_core::game::rules::Rules;
use mind_core::game::rules_event::RulesEpoch;
use mind_core::game::runtime::{CampaignRuntime, run_wave_now, sync_session_with_sim};
use mind_core::game::team::Team;
use mind_core::game::universe::Campaign;
use mind_core::io::fs::{FileSystem, NativeFs};
use mind_core::io::map::MapIo;
use mind_core::io::sim_io::SimIoHandler;
use mind_core::maps::planet::{SectorGenExtras, generate_sector};
use mind_core::maps::shuffle::{GameMode, ShuffleMode};
use mind_core::maps::{Map, Maps};
use mind_core::random::JavaRandom;
use mind_core::sim::unit_runtime::UnitRuntime;
use mind_core::sim::{IoStatus, Sim, SimRuntimeConfig};
use mind_core::world::NoopMapGenHooks;
use mind_core::world::modules::ItemModule;

/// Host failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// The requested map name matched nothing on disk or in the registry.
    UnknownMap(String),
    /// A known built-in/sector-preset map was requested, but the world loader
    /// cannot produce it (no map file and no planet generator).
    NeedsWorldLoader(String),
    /// No world is loaded (the command needs `host` first).
    NoWorld,
    /// A world is already hosted (`ServerControl`: "Already hosting").
    AlreadyHosting,
    /// The world could not be loaded/built (save engine or generator failure).
    World(String),
    /// The plan-21 host/relay call failed (`serve` online mode).
    Net(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::UnknownMap(map) => write!(f, "no map with name `{map}` found"),
            HostError::NeedsWorldLoader(map) => write!(
                f,
                "`{map}` is a known map/sector preset, but no map file or planet \
                 generator can produce it; place a `{map}.msav` under config/maps/"
            ),
            HostError::NoWorld => write!(f, "not hosting. Host a game first"),
            HostError::AlreadyHosting => {
                write!(f, "already hosting. Type 'stop' to stop hosting first")
            }
            HostError::World(reason) => write!(f, "{reason}"),
            HostError::Net(reason) => write!(f, "{reason}"),
        }
    }
}

impl std::error::Error for HostError {}

/// Admin/moderation failure (plan 21 §3.10 seam).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminError {
    /// No live STDB connection: the command needs a database (or offline mode
    /// falls back to the JSON mirror).
    Offline,
    /// Under D2 the action has no representation (IP/subnet bans: no peer IPs).
    Unsupported(String),
    /// The player/identity could not be resolved.
    UnknownTarget(String),
    /// The reducer send failed.
    Connector(String),
    /// No match is currently hosted.
    NoMatch,
}

impl std::fmt::Display for AdminError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdminError::Offline => {
                write!(
                    f,
                    "requires a live STDB connection (plan 21); unavailable offline"
                )
            }
            AdminError::Unsupported(reason) => write!(f, "{reason}"),
            AdminError::UnknownTarget(target) => write!(f, "no player found matching `{target}`"),
            AdminError::Connector(reason) => write!(f, "STDB reducer failed: {reason}"),
            AdminError::NoMatch => write!(f, "no match is hosted"),
        }
    }
}

impl std::error::Error for AdminError {}

/// A live player row for the `players`/`status` view (plan 21 §3.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerView {
    /// Identity hex (64 lowercase hex chars).
    pub identity: String,
    /// Last-known username (`""` when unresolved).
    pub name: String,
    /// Team index, when assigned.
    pub team: Option<u8>,
    /// Whether an admin record exists.
    pub admin: bool,
    /// Whether the member is currently connected (last `relay_member` row).
    pub connected: bool,
    /// Membership role name (`player`/`spectator`).
    pub role: String,
    /// Placeholder for the upstream mobile/modded flags (always false here).
    pub mobile: bool,
    /// Placeholder for locale (`""` when unknown).
    pub locale: String,
}

/// The console-facing admin/moderation seam (plan 21 §3.10 `Admins`).
///
/// The dedicated server implements this over the `mind-stdb` reducer set; the
/// offline mirror is the fallback when no database is reachable (P22-6).
pub trait Admins {
    /// Broadcasts a server `System` chat message (`say`).
    fn say(&mut self, message: &str) -> Result<(), AdminError>;
    /// Kicks a connected player (`kick`).
    fn kick(&mut self, target: &str) -> Result<(), AdminError>;
    /// Bans a player (`ban [kind] <target>`).
    fn ban(&mut self, kind: crate::server::admin::BanKind, target: &str) -> Result<(), AdminError>;
    /// Removes every ban for a target (`unban`).
    fn unban(&mut self, target: &str) -> Result<(), AdminError>;
    /// Removes an identity ban (`pardon`).
    fn pardon(&mut self, target: &str) -> Result<(), AdminError>;
    /// Grants/revokes admin (`admin add|remove`).
    fn grant(&mut self, target: &str, on: bool) -> Result<(), AdminError>;
    /// Enables/disables the global whitelist (`config whitelist`).
    fn set_whitelist_enabled(&mut self, enabled: bool) -> Result<(), AdminError>;
    /// Adds/removes a whitelist identity (`whitelist add|remove`).
    fn whitelist(&mut self, target: &str, on: bool) -> Result<(), AdminError>;
    /// Sets the connection player cap (`playerlimit [off/number]`).
    fn player_limit(&mut self, limit: Option<u16>) -> Result<(), AdminError>;
    /// Offline `search`/`info` scan (mirror + live players).
    fn search(&self, query: &str) -> Vec<PlayerView>;
    /// Snapshot mirrors for `admins`/`bans` listing.
    fn mirror(&self) -> &crate::server::admin::AdminMirror;
}

/// Host state snapshot for `status`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostStatus {
    /// Whether a world is loaded.
    pub hosting: bool,
    /// Loaded map name.
    pub map: Option<String>,
    /// Game mode.
    pub mode: Option<String>,
    /// Current wave.
    pub wave: i32,
    /// Current tick.
    pub tick: u64,
    /// Connected players (plan 21; 0 on a local-only host).
    pub players: usize,
    /// Whole seconds until the next wave (`state.wavetime / 60`).
    pub seconds_until_wave: i32,
    /// The world is paused (`pause on` or auto-pause).
    pub paused: bool,
    /// Auto-pause is currently holding the world.
    pub auto_paused: bool,
    /// The map selected for the restart after a game over.
    pub next_map: Option<String>,
    /// Ticks left before the next map loads (`roundExtraTime`).
    pub restart_in: Option<u64>,
}

/// A host-lifecycle event the server loop logs / relays to the network.
#[derive(Debug, Clone, PartialEq)]
pub enum HostEvent {
    /// A world finished loading and the match is playing.
    Hosted {
        /// Hosted map name.
        map: String,
        /// Gamemode name.
        mode: String,
    },
    /// `Logic.runWave` advanced the wave.
    WaveStarted {
        /// New wave number.
        wave: i32,
    },
    /// The game ended; `next` is the map that will load after `extra_seconds`.
    GameOver {
        /// Wave reached.
        wave: i32,
        /// Winner team id (`rules.waveTeam` for a survival loss).
        winner: u8,
        /// Next map selected by rotation, `None` when the server should close.
        next: Option<String>,
        /// `roundExtraTime` seconds before the reload.
        extra_seconds: f32,
    },
    /// Hosting stopped (game over with no next map, or `stop`).
    Stopped,
}

/// Host configuration derived from the server settings / `--match-config`.
#[derive(Debug, Clone, PartialEq)]
pub struct HostConfig {
    /// `Config.autoPause`: freeze while no player is connected.
    pub auto_pause: bool,
    /// `Config.roundExtraTime` in seconds (12 by default).
    pub round_extra_time: f32,
    /// Deterministic map/world seed (`MatchConfig.map_seed`).
    pub seed: u64,
    /// Ordered map rotation from the match config (empty = shuffle only).
    pub rotation: Vec<String>,
    /// `Maps.ShuffleMode`.
    pub shuffle: ShuffleMode,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            auto_pause: false,
            round_extra_time: 12.0,
            seed: 0,
            rotation: Vec::new(),
            shuffle: ShuffleMode::All,
        }
    }
}

/// The console-facing host seam (plan 21 `HostController` stands behind this).
pub trait HostControl {
    /// Loads `map` in `mode` and starts hosting.
    fn host(&mut self, map: &str, mode: &str) -> Result<(), HostError>;
    /// Stops hosting (world unloaded).
    fn stop(&mut self);
    /// Current host state.
    fn status(&self) -> HostStatus;
    /// Forces the next wave.
    fn run_wave(&mut self) -> Result<(), HostError>;
    /// Live player views (`players`/`status`); empty for a local host.
    fn players(&self) -> Vec<PlayerView> {
        Vec::new()
    }
}

/// A loaded world plus the match session riding on it.
pub struct HostWorld {
    sim: Sim,
    map_name: String,
    mode: Gamemode,
}

impl std::fmt::Debug for HostWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostWorld")
            .field("map", &self.map_name)
            .field("mode", &self.mode.name())
            .field("tick", &self.sim.state.tick)
            .finish()
    }
}

impl HostWorld {
    /// The live match session (`CampaignRuntime.session`), when installed.
    fn session(&self) -> Option<&PlaySession> {
        self.sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .map(|runtime| &runtime.session)
    }

    /// Current wave (`state.wave`).
    fn wave(&self) -> i32 {
        self.session().map_or(0, |session| session.wave)
    }

    /// The one fixed-step tick (`Logic.update`).
    fn tick(&mut self) {
        if let Err(error) = self.sim.tick() {
            log::warn!("host tick failed: {error}");
        }
    }

    /// `Logic.runWave()` on the live session (`Logic.java:319-325`).
    fn run_wave(&mut self) -> Result<i32, HostError> {
        let mut runtime = self
            .sim
            .ecs
            .0
            .remove_resource::<CampaignRuntime>()
            .ok_or(HostError::NoWorld)?;
        let mut unit_runtime = self.sim.ecs.0.get_non_send_mut::<UnitRuntime>();
        run_wave_now(&mut runtime, unit_runtime.as_deref_mut());
        let wave = runtime.session.wave;
        self.sim.ecs.0.insert_resource(runtime);
        Ok(wave)
    }

    /// `state.teams.cores(team)` item fill (`ServerControl.java:790-792`).
    fn fill_items(&mut self, registry: &ContentRegistry, team: u8) -> Result<usize, HostError> {
        let runtime = self
            .sim
            .ecs
            .0
            .remove_resource::<CampaignRuntime>()
            .ok_or(HostError::NoWorld)?;
        let cores: Vec<bevy_ecs::entity::Entity> = runtime
            .session
            .teams
            .get_or_null(TeamId(team))
            .map(|data| data.cores.clone())
            .unwrap_or_default();
        if cores.is_empty() {
            self.sim.ecs.0.insert_resource(runtime);
            return Err(HostError::World(format!(
                "team {} has no cores",
                Team::get(team).name
            )));
        }
        let mut filled = 0usize;
        for entity in cores {
            let capacity = self
                .sim
                .ecs
                .0
                .get::<BuildingComp>(entity)
                .and_then(|building| registry.block(building.block))
                .map(|def| def.item_capacity)
                .unwrap_or(0);
            if capacity <= 0 {
                continue;
            }
            let Some(mut module) = self.sim.ecs.0.get_mut::<ItemModule>(entity) else {
                continue;
            };
            for item in registry.items() {
                module.add(item.id, capacity, capacity);
            }
            filled += 1;
        }
        self.sim.ecs.0.insert_resource(runtime);
        Ok(filled)
    }
}

/// Map rotation (`Maps.setShuffleMode`/`setNextMapOverride`/`getNextMap`).
pub struct Rotation {
    /// Raw ordering installed by `--match-config` (wins over shuffle).
    pub explicit: Vec<String>,
    /// Position inside [`Rotation::explicit`].
    pub cursor: usize,
    /// The `nextmap` override (`Maps.nextMapOverride`).
    pub next: Option<String>,
    /// `Maps.shuffleMode`.
    pub shuffle: ShuffleMode,
    /// Map registry scanned from the data root (`Maps.all`).
    pub maps: Maps,
    /// Deterministic selection RNG (`Mathf.rand`; seeded per host).
    rng: JavaRandom,
}

impl Rotation {
    /// Scans `maps_dir` for `.msav` files and installs `shuffle`.
    pub fn scan(maps_dir: &Path, shuffle: ShuffleMode, seed: u64) -> Self {
        let mut maps = Maps::new();
        maps.shuffle_mode = shuffle;
        let _ = maps.load_from_dir(&NativeFs, maps_dir, true);
        Self {
            explicit: Vec::new(),
            cursor: 0,
            next: None,
            shuffle,
            maps,
            rng: JavaRandom::new(seed),
        }
    }

    /// Number of registry maps.
    pub fn len(&self) -> usize {
        self.maps.len()
    }

    /// Whether no map was found.
    pub fn is_empty(&self) -> bool {
        self.maps.is_empty()
    }

    /// Installs the `--match-config` rotation.
    pub fn set_explicit(&mut self, rotation: Vec<String>) {
        self.explicit = rotation;
        self.cursor = 0;
    }

    /// `shuffle <mode>`: stores and applies the mode.
    pub fn set_shuffle(&mut self, mode: ShuffleMode) {
        self.shuffle = mode;
        self.maps.shuffle_mode = mode;
    }

    /// `nextmap <name>`: pins the next map by name.
    pub fn set_next(&mut self, name: &str) -> bool {
        // Accept explicit names too: a map in `config/maps/` is picked up on
        // the host attempt even before the registry is rescanned.
        self.next = Some(name.to_owned());
        true
    }

    /// Registry map matching `name` (display name, plain name or file stem).
    pub fn find(&self, name: &str) -> Option<&Map> {
        let needle = name.replace('_', " ").to_ascii_lowercase();
        self.maps.all().iter().find(|map| {
            map.name().replace('_', " ").to_ascii_lowercase() == needle
                || map.plain_name().replace('_', " ").to_ascii_lowercase() == needle
                || map
                    .file_stem()
                    .is_some_and(|stem| stem.eq_ignore_ascii_case(name))
        })
    }

    /// `Maps.getNextMap`: override → explicit rotation → shuffle selection.
    pub fn next_map(&mut self, mode: GameMode, current: Option<&str>) -> Option<String> {
        if let Some(name) = self.next.take() {
            return Some(name);
        }
        if !self.explicit.is_empty() {
            // Advance past the current map so a rotation reload never re-hosts
            // the map that just ended.
            let mut index = self.cursor % self.explicit.len();
            if current.is_some_and(|current| self.explicit[index].eq_ignore_ascii_case(current)) {
                index = (index + 1) % self.explicit.len();
            }
            self.cursor = index.wrapping_add(1);
            return Some(self.explicit[index].clone());
        }
        let previous = current.and_then(|name| {
            self.find(name).and_then(|map| {
                self.maps
                    .all()
                    .iter()
                    .position(|entry| entry.file == map.file)
            })
        });
        let index = self.maps.get_next_map(mode, previous, &mut self.rng)?;
        self.maps
            .all()
            .get(index)
            .map(|map| map.file_stem().unwrap_or_else(|| map.name().to_owned()))
    }
}

/// A pending `roundExtraTime` restart (`ServerControl.play(wait, run)`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Restart {
    next_map: Option<String>,
    ticks: u64,
}

/// A local, Godot-free host over a live `mind_core::sim::Sim`.
pub struct LocalHost {
    registry: ContentRegistry,
    maps_dir: PathBuf,
    world: Option<HostWorld>,
    map: Option<String>,
    mode: Option<String>,
    paused: bool,
    auto_pause: bool,
    players: usize,
    round_extra_time: f32,
    restart: Option<Restart>,
    rotation: Rotation,
    events: Vec<HostEvent>,
    seed: u64,
}

impl std::fmt::Debug for LocalHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalHost")
            .field("hosting", &self.world.is_some())
            .field("map", &self.map)
            .field("mode", &self.mode)
            .field("paused", &self.paused)
            .field("players", &self.players)
            .finish()
    }
}

impl LocalHost {
    /// Creates a host over `registry` that looks for map files in `maps_dir`.
    pub fn new(registry: ContentRegistry, maps_dir: impl Into<std::path::PathBuf>) -> Self {
        let maps_dir = maps_dir.into();
        Self {
            registry,
            rotation: Rotation::scan(&maps_dir, ShuffleMode::All, 0),
            maps_dir,
            world: None,
            map: None,
            mode: None,
            paused: false,
            auto_pause: false,
            players: 0,
            round_extra_time: 12.0,
            restart: None,
            events: Vec::new(),
            seed: 0,
        }
    }

    /// The loaded content registry.
    pub fn registry(&self) -> &ContentRegistry {
        &self.registry
    }

    /// Replaces the content registry (`reloadassets`): the next world load
    /// uses the refreshed content; a running world keeps its cloned snapshot.
    pub fn set_registry(&mut self, registry: ContentRegistry) {
        self.registry = registry;
    }

    /// Whether a world is loaded.
    pub fn is_hosting(&self) -> bool {
        self.world.is_some()
    }

    /// The map directory this host scans.
    pub fn maps_dir(&self) -> &Path {
        &self.maps_dir
    }

    /// Applies server settings (`autoPause`/`roundExtraTime`/seed/rotation).
    pub fn configure(&mut self, config: HostConfig) {
        self.auto_pause = config.auto_pause;
        self.round_extra_time = config.round_extra_time.max(0.0);
        self.seed = config.seed;
        self.rotation.set_shuffle(config.shuffle);
        if !config.rotation.is_empty() {
            self.rotation.set_explicit(config.rotation);
        }
    }

    /// Rescans the map directory (`reloadmaps`).
    pub fn reload_maps(&mut self) -> usize {
        let shuffle = self.rotation.shuffle;
        self.rotation = Rotation::scan(&self.maps_dir, shuffle, self.seed);
        self.rotation.len()
    }

    /// The map rotation.
    pub fn rotation(&self) -> &Rotation {
        &self.rotation
    }

    /// Mutable map rotation (`shuffle`/`nextmap`).
    pub fn rotation_mut(&mut self) -> &mut Rotation {
        &mut self.rotation
    }

    /// Connected player count feeding `autoPause`.
    pub fn set_players(&mut self, players: usize) {
        self.players = players;
    }

    /// The connected player count.
    pub fn players_count(&self) -> usize {
        self.players
    }

    /// `pause on/off` (clears auto-pause, `ServerControl.java:687-689`).
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Whether the host loop is holding the world.
    pub fn is_paused(&self) -> bool {
        self.paused || self.auto_paused()
    }

    /// Whether `autoPause` is holding the world now.
    pub fn auto_paused(&self) -> bool {
        if self.world.is_none() {
            return false;
        }
        let pvp = self
            .world
            .as_ref()
            .and_then(HostWorld::session)
            .map(|session| session.rules.pvp)
            .unwrap_or(false);
        super::dedicated::should_pause(self.auto_pause, self.players as u16, pvp)
    }

    /// Drains host lifecycle events (server loop logging / relay).
    pub fn take_events(&mut self) -> Vec<HostEvent> {
        std::mem::take(&mut self.events)
    }

    /// Advances the loaded world by `ticks` fixed steps.
    pub fn tick(&mut self, ticks: u64) {
        for _ in 0..ticks {
            if self.world.is_none() {
                return;
            }
            if let Some(restart) = &mut self.restart {
                restart.ticks = restart.ticks.saturating_sub(1);
                if restart.ticks == 0 {
                    let next = restart.next_map.clone();
                    self.restart = None;
                    match next {
                        Some(map) => self.reload_map(&map),
                        None => self.stop(),
                    }
                }
                continue;
            }
            if self.is_paused() {
                continue;
            }
            if let Some(world) = &mut self.world {
                world.tick();
            }
            self.check_game_over();
        }
    }

    /// Saves the loaded world to `path` (`save <slot>`).
    pub fn save(&mut self, _fs: &dyn FileSystem, path: &Path) -> Result<(), HostError> {
        let world = self.world.as_mut().ok_or(HostError::NoWorld)?;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        world.sim.request_save(path.to_path_buf(), true);
        world.sim.flush_io();
        let responses = world.sim.take_io_responses();
        match responses.first().map(|response| &response.status) {
            Some(IoStatus::Ok) => Ok(()),
            Some(IoStatus::Failed(reason)) => Err(HostError::World(reason.clone())),
            other => {
                log::warn!("save deferred: {other:?}");
                Err(HostError::World("save did not complete".to_owned()))
            }
        }
    }

    /// Loads `path` into a fresh live world (`load <slot>`).
    pub fn load(&mut self, _fs: &dyn FileSystem, path: &Path) -> Result<(), HostError> {
        if !path.exists() {
            return Err(HostError::UnknownMap(path.display().to_string()));
        }
        let mut sim = self.new_sim(1, 1)?;
        // A bare session so the save's rules/wave/stats are restored by the
        // load (`SimIoHandler::load` updates the installed `CampaignRuntime`).
        let campaign = Campaign::from_registry(&self.registry, &EmptyNeighborhood);
        sim.ecs.0.insert_resource(CampaignRuntime::new(
            PlaySession::new(Rules::default()),
            campaign,
        ));
        sim.request_load(path.to_path_buf());
        sim.flush_io();
        let responses = sim.take_io_responses();
        match responses.first().map(|response| &response.status) {
            Some(IoStatus::Ok) => {}
            Some(IoStatus::Failed(reason)) => return Err(HostError::World(reason.clone())),
            other => {
                log::warn!("load deferred: {other:?}");
                return Err(HostError::World("load did not complete".to_owned()));
            }
        }
        let display = path_stem(path);
        sim.set_map_name(display.clone());
        self.sync_cores(&mut sim, false)?;
        if let Some(world) = &mut self.world {
            world.sim = sim;
            world.map_name = display.clone();
        } else {
            self.world = Some(HostWorld {
                sim,
                map_name: display.clone(),
                mode: Gamemode::Survival,
            });
        }
        self.map = Some(display.clone());
        self.mode.get_or_insert_with(|| "survival".to_owned());
        self.events.push(HostEvent::Hosted {
            map: display,
            mode: self.mode.clone().unwrap_or_default(),
        });
        Ok(())
    }

    /// Distinguishes an unknown name from a known built-in/sector preset with
    /// no available world source.
    fn missing_map_error(&self, name: &str) -> HostError {
        let known_builtin = mind_core::maps::DEFAULT_MAP_NAMES.contains(&name);
        if known_builtin || self.registry.sector_by_name(name).is_some() {
            HostError::NeedsWorldLoader(name.to_owned())
        } else {
            HostError::UnknownMap(name.to_owned())
        }
    }

    /// Builds the sim shell, the live building runtime and the save engine.
    fn new_sim(&self, width: i32, height: i32) -> Result<Sim, HostError> {
        let mut sim = Sim::new(self.seed, width, height, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime_from(SimRuntimeConfig::default(), self.registry.clone())
            .map_err(|error| HostError::World(format!("live runtime install failed: {error}")))?;
        let handler = SimIoHandler::boot()
            .map_err(|error| HostError::World(format!("save engine boot failed: {error}")))?;
        sim.set_io_handler(Box::new(handler));
        Ok(sim)
    }

    /// Installs the match session on `sim` (`Logic.play`).
    fn install_session(&self, sim: &mut Sim, rules: Rules) {
        let mut session = PlaySession::new(rules.clone());
        let mut epoch = RulesEpoch::new();
        let _ = play_map(&mut session, rules, None, |_| false, &mut epoch);
        let campaign = Campaign::from_registry(&self.registry, &EmptyNeighborhood);
        sim.ecs
            .0
            .insert_resource(CampaignRuntime::new(session, campaign));
        sim.state.set_phase(State::Playing);
    }

    /// Registers the live cores into the session teams (GAP-9 bridge).
    fn sync_cores(&self, sim: &mut Sim, apply_loadout: bool) -> Result<(), HostError> {
        let spawns = count_spawns(sim, &self.registry);
        let mut runtime = sim
            .ecs
            .0
            .remove_resource::<CampaignRuntime>()
            .ok_or(HostError::NoWorld)?;
        let _ = sync_session_with_sim(
            &mut runtime.session,
            &mut sim.ecs.0,
            &self.registry,
            spawns,
            apply_loadout,
        );
        runtime.session.phase = State::Playing;
        if runtime.session.wave == 0 {
            runtime.session.wave = 1;
        }
        sim.ecs.0.insert_resource(runtime);
        sim.state.set_phase(State::Playing);
        Ok(())
    }

    /// Reloads `map` for the `roundExtraTime` restart (`ServerControl.play`).
    fn reload_map(&mut self, map: &str) {
        let mode = self
            .mode
            .clone()
            .unwrap_or_else(|| Gamemode::Survival.name().to_owned());
        match self.load_world(map, &mode) {
            Ok(world) => {
                let display = world.map_name.clone();
                self.world = Some(world);
                self.map = Some(display.clone());
                self.events.push(HostEvent::Hosted { map: display, mode });
            }
            Err(error) => {
                log::error!("could not load next map `{map}`: {error}");
                self.stop();
            }
        }
    }

    /// Loads one world without installing it on the host.
    fn load_world(&mut self, map: &str, mode: &str) -> Result<HostWorld, HostError> {
        let gamemode = Gamemode::from_name(mode).unwrap_or(Gamemode::Survival);
        let name = if map.trim().is_empty() {
            "synthetic"
        } else {
            map
        };
        if name == "synthetic" {
            let mut sim = self.new_sim(64, 64)?;
            sim.grid.fill(BlockId::AIR, BlockId::AIR);
            let mut rules = Rules {
                waves: true,
                ..Rules::default()
            };
            gamemode.apply(&mut rules);
            self.install_session(&mut sim, rules);
            // A core so the synthetic fixture is a playable survival map.
            if let Some(core) = self.registry.block_id("core-shard") {
                let _ = sim.apply(Command::Place {
                    x: 32,
                    y: 32,
                    block: core,
                });
            }
            self.sync_cores(&mut sim, true)?;
            sim.set_map_name(name);
            return Ok(HostWorld {
                sim,
                map_name: name.to_owned(),
                mode: gamemode,
            });
        }

        if let Some(path) = self.resolve_map_file(name) {
            let mut sim = self.new_sim(1, 1)?;
            sim.request_load(path.clone());
            sim.flush_io();
            let responses = sim.take_io_responses();
            match responses.first().map(|response| &response.status) {
                Some(IoStatus::Ok) => {}
                Some(IoStatus::Failed(reason)) => return Err(HostError::World(reason.clone())),
                other => {
                    log::warn!("load deferred for `{name}`: {other:?}");
                    return Err(HostError::World(format!(
                        "map `{name}` did not load through the save engine"
                    )));
                }
            }
            let display = path_stem(&path);
            sim.set_map_name(display.clone());
            let rules = self.map_rules(&path, gamemode);
            self.install_session(&mut sim, rules);
            self.sync_cores(&mut sim, true)?;
            return Ok(HostWorld {
                sim,
                map_name: display,
                mode: gamemode,
            });
        }

        if let Some(preset) = self.registry.sector_by_name(name) {
            let planet = self
                .registry
                .planet(preset.planet)
                .map(|planet| planet.name.clone())
                .ok_or_else(|| HostError::NeedsWorldLoader(name.to_owned()))?;
            let sector = preset.sector;
            let generated =
                generate_sector(&planet, sector, &self.registry, SectorGenExtras::default())
                    .ok_or_else(|| HostError::NeedsWorldLoader(name.to_owned()))?;
            let mind_core::maps::planet::GeneratedSector {
                grid,
                rules,
                mut generator,
                ..
            } = generated;
            let (width, height) = (grid.tiles.width, grid.tiles.height);
            let mut sim = self.new_sim(width, height)?;
            sim.grid = grid;
            let mut rules = rules;
            gamemode.apply(&mut rules);
            if rules.default_team == 0 {
                rules.default_team = 1;
            }
            if rules.wave_team == 0 {
                rules.wave_team = 2;
            }
            // Launch loadout (`Universe.getLastLoadout`): the player core.
            let mut schematics = mind_core::game::schematics::Schematics::new();
            schematics.load_loadouts(&self.registry);
            let shard = self.registry.block_id("core-shard");
            let copper = self.registry.item_id("copper");
            let loadout = shard.and_then(|shard| {
                let index = schematics.get_default_loadout(shard)?;
                Some(mind_core::maps::generators::planet::LaunchLoadout {
                    schematic: schematics.all[index].clone(),
                    stacks: copper.map(|copper| vec![(copper, 100)]).unwrap_or_default(),
                })
            });
            let player_team = rules.default_team;
            let wave_team = rules.wave_team;
            {
                let mut ctx = mind_core::maps::generators::planet::StructureCtx {
                    sim: &mut sim,
                    content: &self.registry,
                    bases: None,
                    loadout: loadout.as_ref(),
                    rules: &mut rules,
                    player_team,
                    wave_team,
                };
                generator.place_structures(&mut ctx);
            }
            self.install_session(&mut sim, rules);
            self.sync_cores(&mut sim, true)?;
            sim.set_map_name(name);
            return Ok(HostWorld {
                sim,
                map_name: name.to_owned(),
                mode: gamemode,
            });
        }

        Err(self.missing_map_error(name))
    }

    /// Reads the map's own rules and applies the gamemode preset
    /// (`map.applyRules(mode)`; `ServerControl.java:551-555`).
    fn map_rules(&self, path: &Path, mode: Gamemode) -> Rules {
        let mut rules = match MapIo::create_map(&NativeFs, path, true) {
            Ok(header) => {
                let map = Map::from_header(&header, true);
                map.rules(Rules::default(), &NoopMapGenHooks)
            }
            Err(error) => {
                log::warn!("could not read map rules for {}: {error}", path.display());
                Rules::default()
            }
        };
        mode.apply(&mut rules);
        rules
    }

    /// Resolves `name` to a `.msav` file below `maps_dir` (or `default/`).
    fn resolve_map_file(&self, name: &str) -> Option<PathBuf> {
        if name.contains('/') || name.contains('\\') {
            let direct = PathBuf::from(name);
            if direct.is_file() {
                return Some(direct);
            }
        }
        let candidate = self.maps_dir.join(name);
        let candidate = if candidate.extension().is_some() {
            candidate
        } else {
            candidate.with_extension("msav")
        };
        if candidate.is_file() {
            return Some(candidate);
        }
        let default = self
            .maps_dir
            .join("default")
            .join(name)
            .with_extension("msav");
        if default.is_file() {
            return Some(default);
        }
        self.rotation
            .find(name)
            .map(|map| map.file.clone())
            .filter(|file| file.is_file())
    }

    /// `Logic.checkGameState` result handling: the game-over listener
    /// (`ServerControl.java:116-133`).
    fn check_game_over(&mut self) {
        let over = self
            .world
            .as_ref()
            .and_then(HostWorld::session)
            .map(|session| session.game_over)
            .unwrap_or(false);
        if over && self.restart.is_none() {
            self.game_over_transition();
        }
    }

    /// Selects the next map and schedules the `roundExtraTime` restart.
    fn game_over_transition(&mut self) {
        let (wave, winner, mode) = match self.world.as_ref().and_then(HostWorld::session) {
            Some(session) => {
                let _ = (session.rules.pvp, session.rules.attack_mode);
                (
                    session.wave,
                    session.rules.wave_team,
                    self.world.as_ref().map(|w| w.mode),
                )
            }
            None => return,
        };
        let game_mode = mode.map(to_game_mode).unwrap_or(GameMode::Survival);
        let next = self.rotation.next_map(game_mode, self.map.as_deref());
        let extra = self.round_extra_time;
        let ticks = (extra * crate::server::TICKS_PER_SECOND as f32).ceil() as u64;
        if let Some(map) = &next {
            log::info!("game over: next map is `{map}` in {extra} seconds");
        }
        self.restart = Some(Restart {
            next_map: next.clone(),
            ticks,
        });
        self.events.push(HostEvent::GameOver {
            wave,
            winner,
            next,
            extra_seconds: extra,
        });
    }

    /// Forced game over (`gameover` command; `ServerControl.java:1240-1243`).
    pub fn game_over(&mut self) -> Result<(), HostError> {
        let world = self.world.as_mut().ok_or(HostError::NoWorld)?;
        let winner = world
            .session()
            .map(|session| session.rules.wave_team)
            .unwrap_or(2);
        let Some(mut runtime) = world.sim.ecs.0.remove_resource::<CampaignRuntime>() else {
            return Err(HostError::NoWorld);
        };
        runtime.session.game_over = true;
        runtime.session.stats.waves_lasted = runtime.session.wave;
        world.sim.ecs.0.insert_resource(runtime);
        log::info!("core destroyed (winner team {winner})");
        self.game_over_transition();
        Ok(())
    }

    /// Fills a team's cores with every item (`fillitems [team]`).
    pub fn fill_items(&mut self, team_name: Option<&str>) -> Result<usize, HostError> {
        let team = match team_name {
            Some(name) if !name.trim().is_empty() => Team::all()
                .iter()
                .find(|team| team.name.eq_ignore_ascii_case(name.trim()))
                .map(|team| team.id)
                .ok_or_else(|| HostError::World(format!("no team with name `{name}` found")))?,
            _ => Team::get(1).id,
        };
        let world = self.world.as_mut().ok_or(HostError::NoWorld)?;
        world.fill_items(&self.registry, team)
    }
}

impl HostControl for LocalHost {
    fn host(&mut self, map: &str, mode: &str) -> Result<(), HostError> {
        if self.world.is_some() {
            return Err(HostError::AlreadyHosting);
        }
        let world = self.load_world(map, mode)?;
        self.map = Some(world.map_name.clone());
        self.mode = Some(world.mode.name().to_owned());
        self.paused = false;
        self.restart = None;
        self.events.push(HostEvent::Hosted {
            map: world.map_name.clone(),
            mode: world.mode.name().to_owned(),
        });
        self.world = Some(world);
        Ok(())
    }

    fn stop(&mut self) {
        if self.world.take().is_some() {
            self.events.push(HostEvent::Stopped);
        }
        self.map = None;
        self.mode = None;
        self.paused = false;
        self.restart = None;
    }

    fn status(&self) -> HostStatus {
        match &self.world {
            Some(world) => HostStatus {
                hosting: true,
                map: self.map.clone(),
                mode: self.mode.clone(),
                wave: world.wave(),
                tick: world.sim.state.tick,
                players: self.players,
                seconds_until_wave: world
                    .session()
                    .map(|session| (session.wavetime / 60.0) as i32)
                    .unwrap_or(0),
                paused: self.is_paused(),
                auto_paused: self.auto_paused(),
                next_map: self
                    .restart
                    .as_ref()
                    .and_then(|restart| restart.next_map.clone()),
                restart_in: self.restart.as_ref().map(|restart| restart.ticks),
            },
            None => HostStatus {
                players: self.players,
                ..HostStatus::default()
            },
        }
    }

    fn run_wave(&mut self) -> Result<(), HostError> {
        let world = self.world.as_mut().ok_or(HostError::NoWorld)?;
        let wave = world.run_wave()?;
        self.events.push(HostEvent::WaveStarted { wave });
        Ok(())
    }
}

/// Game mode a map is valid for (`maps::shuffle::GameMode`).
fn to_game_mode(mode: Gamemode) -> GameMode {
    match mode {
        Gamemode::Survival => GameMode::Survival,
        Gamemode::Sandbox => GameMode::Sandbox,
        Gamemode::Attack => GameMode::Attack,
        Gamemode::Pvp => GameMode::Pvp,
        Gamemode::Editor => GameMode::Editor,
    }
}

/// Counts the map's enemy spawn tiles (`WaveSpawner.countSpawns`).
fn count_spawns(sim: &Sim, registry: &ContentRegistry) -> i32 {
    sim.grid
        .tiles
        .array()
        .iter()
        .filter(|tile| {
            registry
                .block(tile.overlay)
                .is_some_and(|def| def.kind == BlockKind::SpawnBlock)
        })
        .count() as i32
}

/// File stem of `path`, falling back to the whole file name.
fn path_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("loaded")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ContentRegistry {
        crate::server::boot_content().expect("content boot")
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mind-host-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create maps dir");
        dir
    }

    fn fixture_map(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../mind-core/tests/fixtures/maps")
            .join(format!("{name}.msav"))
    }

    fn copy_fixture(dir: &Path, name: &str) -> PathBuf {
        let target = dir.join(format!("{name}.msav"));
        std::fs::copy(fixture_map(name), &target).expect("copy fixture map");
        target
    }

    fn host_with(tag: &str, config: HostConfig) -> (LocalHost, PathBuf) {
        let dir = scratch(tag);
        let mut host = LocalHost::new(registry(), dir.clone());
        host.configure(config);
        (host, dir)
    }

    /// `host synthetic` builds a real live sim (not the plan-04 fixture).
    #[test]
    fn synthetic_host_builds_a_live_sim() {
        let (mut host, dir) = host_with("synthetic", HostConfig::default());
        host.host("synthetic", "survival").expect("host");
        let status = host.status();
        assert!(status.hosting);
        assert_eq!(status.map.as_deref(), Some("synthetic"));
        assert_eq!(status.mode.as_deref(), Some("survival"));
        assert!(status.wave >= 1);
        assert_eq!(status.players, 0);

        let before = host.status().tick;
        host.tick(30);
        assert_eq!(host.status().tick, before + 30, "the world ticks");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A real `.msav` map loads through the live-sim save engine and brings
    /// its own tiles + rules (the former `NeedsWorldLoader` case).
    #[test]
    fn msav_map_loads_with_real_tiles_and_rules() {
        let (mut host, dir) = host_with("msav", HostConfig::default());
        copy_fixture(&dir, "fork");
        host.host("fork", "survival").expect("host map file");
        let world = host.world.as_ref().expect("world");
        let expected = MapIo::create_map(&NativeFs, &fixture_map("fork"), true).expect("header");
        assert_eq!(world.sim.grid.width(), expected.width);
        assert_eq!(world.sim.grid.height(), expected.height);
        assert!(
            world
                .sim
                .grid
                .tiles
                .array()
                .iter()
                .any(|tile| tile.block != BlockId::AIR),
            "the map's buildings were materialized"
        );
        assert!(host.status().wave >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `runwave` runs the real `Logic.runWave` and advances the session wave.
    #[test]
    fn runwave_advances_the_session_wave() {
        let (mut host, dir) = host_with("wave", HostConfig::default());
        host.host("synthetic", "survival").expect("host");
        let before = host.status().wave;
        host.run_wave().expect("run wave");
        assert_eq!(host.status().wave, before + 1);
        assert!(matches!(
            host.take_events().last(),
            Some(HostEvent::WaveStarted { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `fillitems` fills a team's live core module with every item.
    #[test]
    fn fillitems_fills_the_live_core() {
        let (mut host, dir) = host_with("fill", HostConfig::default());
        host.host("synthetic", "survival").expect("host");
        let filled = host.fill_items(None).expect("fillitems sharded");
        assert_eq!(filled, 1, "the synthetic core is filled");
        let core = host
            .world
            .as_ref()
            .and_then(|world| world.session())
            .and_then(|session| {
                session
                    .teams
                    .get_or_null(TeamId(1))
                    .and_then(|data| data.cores.first().copied())
            })
            .expect("core entity");
        let module = host
            .world
            .as_ref()
            .expect("world")
            .sim
            .ecs
            .0
            .get::<ItemModule>(core)
            .expect("item module");
        let copper = registry().item_id("copper").expect("copper");
        assert!(module.get(copper) > 0, "copper filled");
        assert!(
            host.fill_items(Some("no-such-team")).is_err(),
            "unknown team rejected"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `autoPause` holds the world with no players and releases it on join.
    #[test]
    fn auto_pause_holds_and_releases_the_world() {
        let config = HostConfig {
            auto_pause: true,
            ..HostConfig::default()
        };
        let (mut host, dir) = host_with("autopause", config);
        host.host("synthetic", "survival").expect("host");
        let before = host.status().tick;
        host.tick(30);
        assert_eq!(host.status().tick, before, "no players: paused");
        assert!(host.status().auto_paused);
        host.set_players(1);
        host.tick(30);
        assert_eq!(host.status().tick, before + 30, "player joined: playing");
        assert!(!host.status().auto_paused);
        host.set_paused(true);
        let held = host.status().tick;
        host.tick(10);
        assert_eq!(host.status().tick, held, "manual pause holds too");
        host.set_paused(false);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A game over selects the next rotation map and reloads after
    /// `roundExtraTime` seconds (`ServerControl.play`).
    #[test]
    fn gameover_rotation_and_round_extra_time() {
        let dir = scratch("gameover");
        copy_fixture(&dir, "fork");
        copy_fixture(&dir, "caldera");
        let config = HostConfig {
            round_extra_time: 1.0,
            rotation: vec!["fork".to_owned(), "caldera".to_owned()],
            ..HostConfig::default()
        };
        let mut host = LocalHost::new(registry(), dir.clone());
        host.configure(config);
        host.host("fork", "survival").expect("host fork");
        host.game_over().expect("game over");
        let status = host.status();
        assert_eq!(status.next_map.as_deref(), Some("caldera"));
        assert_eq!(status.restart_in, Some(60));
        // 59 ticks: still on `fork`; the 60th loads `caldera`.
        host.tick(59);
        assert_eq!(host.status().map.as_deref(), Some("fork"));
        host.tick(1);
        assert_eq!(host.status().map.as_deref(), Some("caldera"));
        assert!(host.take_events().iter().any(|event| matches!(
            event,
            HostEvent::GameOver { next: Some(map), .. } if map == "caldera"
        )));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With no rotation candidate the game-over path stops hosting.
    #[test]
    fn gameover_without_maps_stops_hosting() {
        let (mut host, dir) = host_with("nomaps", HostConfig::default());
        host.host("synthetic", "survival").expect("host");
        host.game_over().expect("game over");
        assert_eq!(host.status().next_map, None);
        host.tick(12 * 60);
        assert!(!host.status().hosting, "no next map: hosting stops");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `nextmap` pins the next map and `shuffle` changes the selection mode.
    #[test]
    fn nextmap_and_shuffle_configure_rotation() {
        let (mut host, dir) = host_with("rotation", HostConfig::default());
        copy_fixture(&dir, "fork");
        copy_fixture(&dir, "caldera");
        let len = host.reload_maps();
        assert_eq!(len, 2, "both custom maps scanned");
        assert!(host.rotation_mut().set_next("caldera"));
        host.host("synthetic", "survival").expect("host");
        host.game_over().expect("game over");
        assert_eq!(host.status().next_map.as_deref(), Some("caldera"));
        host.rotation_mut().set_shuffle(ShuffleMode::None);
        assert_eq!(host.rotation().shuffle, ShuffleMode::None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A hosted world saves and loads through the live save engine.
    #[test]
    fn hosted_world_saves_and_loads() {
        let dir = scratch("saveload");
        let mut host = LocalHost::new(registry(), dir.clone());
        host.host("synthetic", "survival").expect("host");
        let save = dir.join("1.msav");
        host.save(&NativeFs, &save).expect("save");
        assert!(save.is_file(), "save file written");
        host.stop();
        host.load(&NativeFs, &save).expect("load");
        assert!(host.status().hosting);
        assert!(host.status().wave >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A campaign sector preset generates a real world + rules (`generate_sector`
    /// + `place_structures`), closing the former `NeedsWorldLoader` case.
    #[test]
    fn sector_preset_generates_a_playable_world() {
        let (mut host, dir) = host_with("sector", HostConfig::default());
        host.host("groundZero", "survival")
            .expect("host sector preset");
        let world = host.world.as_ref().expect("world");
        assert!(
            world.sim.grid.width() > 8 && world.sim.grid.height() > 8,
            "generated sector grid"
        );
        assert!(
            world
                .sim
                .grid
                .tiles
                .array()
                .iter()
                .any(|tile| tile.block != BlockId::AIR),
            "sector structures/terrain placed"
        );
        let status = host.status();
        assert_eq!(status.map.as_deref(), Some("groundZero"));
        assert!(status.wave >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Hosting twice is the upstream `Already hosting` error.
    #[test]
    fn hosting_twice_is_rejected() {
        let (mut host, dir) = host_with("twice", HostConfig::default());
        host.host("synthetic", "survival").expect("host");
        assert_eq!(
            host.host("synthetic", "survival").unwrap_err(),
            HostError::AlreadyHosting
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
