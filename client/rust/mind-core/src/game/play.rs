// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Campaign play flows, game-over and sector capture (plan 12 M8).
//!
//! Ported from `core/src/mindustry/core/Control.java`
//! (`playMap`/`playSector`/`playNewSector`), `core/src/mindustry/core/Logic.java`
//! (`play`/`runWave`/`checkGameState`/`sectorCapture`/`gameOver`/`updateGameOver`)
//! and the campaign half of `Logic.play`.
//!
//! The live world (`world.loadSector`/`loadMap`) and the entity groups are plan
//! 06/07/11; this module owns the **deterministic state machine** around them:
//! it installs rules, fires the ordered events, runs campaign difficulty
//! scaling, and decides capture/loss/game-over. The [`PlaySession`] is the
//! plan-12 half of `GameState` (`stats`/`teams`/`markers`/`objectives`/`wave`/
//! `wavetime`/`gameOver`/`won`/`enemies`/`sector`); the base phase/tick header
//! stays plan 05's [`crate::game::State`].

use super::campaign_rules::CampaignApply;
use super::map_markers::MapMarkers;
use super::map_objectives::MapObjectivesRuntime;
use super::planet::Planet;
use super::rules::Rules;
use super::rules_event::{RulesEpoch, apply_rules_load};
use super::sector::{CoreSnapshot, Sector, SectorInfoState};
use super::teams::Teams;
use super::tech_tree;
use super::universe::Campaign;
use crate::content::{ContentRef, ContentRegistry, ContentType, PlanetId};
use crate::ecs::TeamId;
use crate::event::{self as events, SimEvent};
use crate::game::State;
use crate::io::SaveMeta;
use crate::io::json::content_serde::SectorKey;
use crate::io::json::rules::{GameStats, SectorInfo};
use crate::io::settings::SettingsStore;

/// `(planet, dense sector id)` runtime handle.
pub type SectorRef = (PlanetId, u16);

/// Ordered play-flow / game-over events (plan 12 §3.2 registration list).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayEvent {
    /// `PlayEvent` (`Logic.play`).
    Play,
    /// `WaveEvent` (`Logic.runWave`).
    Wave,
    /// `SectorLaunchEvent` (`Control.playNewSector`).
    SectorLaunch {
        /// Launched planet.
        planet: PlanetId,
        /// Launched sector.
        sector: u16,
    },
    /// `SectorCaptureEvent` (`Logic.sectorCapture`).
    SectorCapture {
        /// Captured planet.
        planet: PlanetId,
        /// Captured sector.
        sector: u16,
        /// First-ever capture (`!wasCaptured` before).
        initial: bool,
    },
    /// `Control`'s last-sector victory (`ui.campaignComplete.show`).
    CampaignComplete {
        /// Completed planet.
        planet: PlanetId,
        /// Final sector.
        sector: u16,
    },
    /// `SectorLoseEvent` (campaign loss).
    SectorLose {
        /// Lost planet.
        planet: PlanetId,
        /// Lost sector.
        sector: u16,
        /// Winning team.
        winner: u8,
    },
    /// `GameOverEvent` (`Logic.gameOver`/`updateGameOver`).
    GameOver {
        /// Winning team.
        winner: u8,
    },
    /// `Trigger.newGame`.
    NewGame,
    /// `RulesLoadEvent` (`fromSave` + epoch).
    RulesLoad {
        /// Whether the rules came from a save.
        from_save: bool,
        /// Rules revision after the load.
        rules_epoch: u32,
    },
    /// `Call.setRules` relay emitted for an in-game rules edit.
    SetRules {
        /// Rules revision after the edit.
        rules_epoch: u32,
    },
}

/// The plan-12 half of `GameState` plus the live match config.
#[derive(Debug, Clone)]
pub struct PlaySession {
    /// Coarse phase (plan 05).
    pub phase: State,
    /// Active match rules.
    pub rules: Rules,
    /// Team registry/caches.
    pub teams: Teams,
    /// Map markers.
    pub markers: MapMarkers,
    /// Objective executor.
    pub objectives: MapObjectivesRuntime,
    /// Per-match statistics.
    pub stats: GameStats,
    /// Current wave.
    ///
    /// Java's `GameState.wave` starts at 1 (`GameState.java:17`) and
    /// `Logic.runWave` increments it AFTER the spawner consumed
    /// `wave - 1` (`WaveSpawner.java:74`).
    pub wave: i32,
    /// Ticks until the next wave.
    pub wavetime: f32,
    /// `state.gameOver`.
    pub game_over: bool,
    /// `state.afterGameOver`.
    pub after_game_over: bool,
    /// `state.won` (player team won).
    pub won: bool,
    /// Live enemy count (`state.enemies`).
    pub enemies: i32,
    /// Enemy spawn-point count (`spawner.countSpawns`).
    pub spawn_count: i32,
    /// Whether the wave spawner is mid-spawn (`spawner.isSpawning`).
    pub is_spawning: bool,
    /// `sector.preset.attackAfterWaves` for the current sector.
    pub attack_after_waves: bool,
    /// Current campaign sector, if any (`state.rules.sector`).
    pub sector: Option<SectorRef>,
    /// Campaign difficulty wave-time multiplier (1.0 outside campaign).
    pub difficulty_wave_time_multiplier: f32,
    /// `planet.campaignRules.difficulty.enemySpawnMultiplier` (1.0 outside
    /// campaign; `WaveSpawner.spawnEnemies` campaign count scaling).
    pub difficulty_enemy_spawn_multiplier: f32,
    /// Whether the player core loadout is applied at launch.
    pub allow_launch_loadout: bool,
    /// Whether the sector preset adds starting items.
    pub add_starting_items: bool,
    /// `PlayEvent` weather cooldown re-shuffle pending (`Logic.java:90-99`):
    /// set by every `logic_play` and consumed once by `weather_system`.
    pub weather_reset_pending: bool,
    /// Typed event rows queued by the play flows for the host to dispatch onto
    /// the [`Events`](crate::event::Events) bus (the port's `Events.fire`
    /// seam, GAPS #37). Bounded by
    /// [`EVENT_QUEUE_CAPACITY`](crate::event::EVENT_QUEUE_CAPACITY); a host
    /// drains it with [`Self::take_event_rows`] and feeds
    /// [`Events::dispatch_row`](crate::event::Events::dispatch_row).
    pub event_rows: Vec<events::EventRow>,
}

impl Default for PlaySession {
    fn default() -> Self {
        Self::new(Rules::default())
    }
}

impl PlaySession {
    /// Creates a menu-phase session with the given rules.
    pub fn new(rules: Rules) -> Self {
        Self {
            phase: State::Menu,
            rules,
            teams: Teams::new(),
            markers: MapMarkers::new(),
            objectives: MapObjectivesRuntime::new(),
            stats: GameStats::default(),
            wave: 1,
            wavetime: 0.0,
            game_over: false,
            after_game_over: false,
            won: false,
            enemies: 0,
            spawn_count: 0,
            is_spawning: false,
            attack_after_waves: false,
            sector: None,
            difficulty_wave_time_multiplier: 1.0,
            difficulty_enemy_spawn_multiplier: 1.0,
            allow_launch_loadout: false,
            add_starting_items: false,
            weather_reset_pending: false,
            event_rows: Vec::new(),
        }
    }

    /// Queues one typed event row for the host's `Events` bus.
    ///
    /// The outbox drops the oldest row when a host never drains it
    /// ([`EVENT_QUEUE_CAPACITY`](crate::event::EVENT_QUEUE_CAPACITY)): the
    /// rows are a replay window, not owned state.
    pub fn push_event_row(&mut self, event: events::EventRow) {
        if self.event_rows.len() >= crate::event::EVENT_QUEUE_CAPACITY {
            self.event_rows.remove(0);
        }
        self.event_rows.push(event);
    }

    /// Drains the queued typed rows (host `Events.dispatch` input).
    pub fn take_event_rows(&mut self) -> Vec<events::EventRow> {
        std::mem::take(&mut self.event_rows)
    }

    /// Queued typed rows, oldest first.
    pub fn event_rows(&self) -> &[events::EventRow] {
        &self.event_rows
    }

    /// `state.isCampaign()`.
    pub fn is_campaign(&self) -> bool {
        self.sector.is_some()
    }

    /// `state.rules.defaultTeam`.
    pub fn default_team(&self) -> u8 {
        self.rules.default_team
    }

    /// `state.rules.waveTeam`.
    pub fn wave_team(&self) -> u8 {
        self.rules.wave_team
    }

    /// `state.teams.playerCores().size`.
    pub fn player_core_count(&self) -> usize {
        self.teams
            .get_or_null(TeamId(self.default_team()))
            .map_or(0, |data| data.cores.len())
    }

    /// `state.teams.cores(waveTeam).size`.
    pub fn wave_core_count(&self) -> usize {
        self.teams
            .get_or_null(TeamId(self.wave_team()))
            .map_or(0, |data| data.cores.len())
    }

    /// `waveTeam.isAlive()`.
    pub fn wave_team_alive(&self) -> bool {
        self.wave_core_count() > 0
    }

    /// `logic.reset()` world half: recreate the transient match state.
    pub fn reset_world(&mut self) {
        self.phase = State::Menu;
        self.teams = Teams::new();
        self.markers.clear();
        self.objectives.clear();
        self.stats.reset();
        self.wave = 1;
        self.wavetime = 0.0;
        self.game_over = false;
        self.after_game_over = false;
        self.won = false;
        self.enemies = 0;
        self.spawn_count = 0;
        self.is_spawning = false;
        self.sector = None;
    }

    /// `SaveSlot.load` meta half (`Saves.java:314-328`): the saved rules/wave,
    /// wave timer and stats replace the live session's, and the loaded match is
    /// playing (`readMeta`/`readRules`).
    ///
    /// Extracted from the free `apply_loaded_meta` (`mind-gdext`
    /// `campaign.rs:7323-7355` at `291e30a8`; adopted by X01).
    pub fn apply_save_meta(&mut self, meta: &SaveMeta) {
        if let Ok(rules) =
            serde_json::from_str::<Rules>(&crate::io::json::quote_bare_keys(&meta.rules_json))
        {
            self.rules = rules;
        } else {
            log::warn!("load: rules JSON did not parse; session rules kept");
        }
        self.wave = meta.wave;
        if let Some(wavetime) = meta
            .tags
            .get("wavetime")
            .and_then(|value| value.parse::<f32>().ok())
        {
            self.wavetime = wavetime;
        }
        if let Some(stats) = meta
            .tags
            .get("stats")
            .and_then(|value| serde_json::from_str::<GameStats>(value).ok())
        {
            self.stats = stats;
        }
        self.phase = State::Playing;
    }
}

/// `Control.playMap(map, rules)` campaign half.
///
/// `retain_from` is the map's own rules whose content fields are forced onto
/// `incoming` (invariant 6: exactly once, before `RulesLoadEvent`).
pub fn play_map(
    session: &mut PlaySession,
    incoming: Rules,
    retain_from: Option<&Rules>,
    is_patch_content: impl Fn(&str) -> bool,
    epoch: &mut RulesEpoch,
) -> Vec<PlayEvent> {
    session.reset_world();
    let mut rules = incoming;
    if let Some(source) = retain_from {
        rules.retain_content_fields(source, is_patch_content);
    }
    rules.sector = None;
    rules.editor = false;
    session.rules = rules;
    session.sector = None;
    let incoming = session.rules.clone();
    let load = apply_rules_load(&mut session.rules, incoming, false, epoch);
    let mut events = vec![PlayEvent::RulesLoad {
        from_save: false,
        rules_epoch: load.rules_epoch,
    }];
    // `World.loadMap` -> `World.endMapLoad` fires `WorldLoadEvent` before
    // `Logic.play` fires `PlayEvent` (`World.java:220-230`, `Logic.java:105-125`).
    session.push_event_row(events::EventRow::Game(events::GameEvent::WorldLoadEvent(
        events::WorldLoadEvent,
    )));
    events.extend(logic_play(session));
    events.push(PlayEvent::NewGame);
    events
}

/// `World.setSectorRules` campaign half: folds the sector preset (capture wave,
/// waves/attack mode) and the planet/campaign rules into the session rules.
///
/// Returns `true` when the sector has a resolvable preset. Fresh matches start
/// from neutral wave/attack state because `logic.reset` recreates
/// `GameState.rules` upstream, while this port keeps the `Rules` value across
/// launches.
pub fn apply_sector_preset_rules(
    session: &mut PlaySession,
    campaign: &Campaign,
    registry: &ContentRegistry,
    planet: PlanetId,
    sector_id: u16,
) -> bool {
    session.rules.win_wave = 0;
    session.rules.waves = false;
    session.rules.attack_mode = false;
    session.rules.disable_world_processors = false;
    session.rules.objective_timer_multiplier = 1.0;
    session.attack_after_waves = false;
    session.add_starting_items = false;
    session.allow_launch_loadout = false;

    let Some(planet_record) = campaign.planet(planet) else {
        return false;
    };
    let mut applied = false;
    if let Some(sector) = planet_record.sector(sector_id)
        && let Some(preset_id) = sector.preset
        && let Some(preset) = registry.sector(preset_id)
    {
        session.rules.win_wave = preset.capture_wave;
        let attack = preset.capture_wave <= 0 && sector.has_enemy_base();
        session.rules.attack_mode = attack;
        session.rules.waves = !attack;
        if session.rules.win_wave <= 0 && !attack && planet_record.allow_waves {
            // `SectorInfo.write`: infinite waves get a default win wave.
            session.rules.win_wave = 30;
        }
        session.attack_after_waves = preset.attack_after_waves;
        session.add_starting_items = preset.add_starting_items;
        session.allow_launch_loadout =
            sector.allow_launch_loadout(planet_record.allow_launch_loadout);
        if preset.no_lighting {
            session.rules.lighting = false;
        }
        applied = true;
    }

    // `Planet.applyRules(rules, customGame=false)`: env/attributes + campaign
    // fold (fog, hideSpawns, difficulty team multipliers).
    planet_record.apply_rules(registry, &mut session.rules, false, false);
    session.difficulty_wave_time_multiplier = planet_record
        .campaign_rules
        .difficulty
        .wave_time_multiplier();
    session.difficulty_enemy_spawn_multiplier = planet_record
        .campaign_rules
        .difficulty
        .enemy_spawn_multiplier();
    applied
}

/// Whether `Logic.play`'s starting-items half applies the launch loadout to the
/// default team's core (`Logic.java:277`), extended with the port's launcher
/// launch (see [`apply_launch_loadout`]).
pub fn launch_loadout_applies(session: &PlaySession) -> bool {
    !session.is_campaign()
        || !session.allow_launch_loadout
        || !session.rules.loadout.is_empty()
        || session.add_starting_items
}

/// `Logic.play` starting-items half: clear the default team's core inventory
/// and add the launch loadout. Core storage capacity is a plan-08/11 runtime
/// concern here, so the add is unbounded.
pub fn apply_launch_loadout(session: &mut PlaySession, registry: &ContentRegistry) {
    // `Logic.play`'s condition (`Logic.java:277`) covers custom games,
    // planets that disallow launch loadouts and presets that add starting
    // items. Java's launcher launch applies the chosen loadout through the
    // generator (`Schematics.placeLaunchLoadout`); the port has no generator
    // loadout seam yet, so an explicit non-empty `rules.loadout` (set by
    // `start_sector_with_loadout`) rides this path instead (PAR-0085).
    if !launch_loadout_applies(session) {
        return;
    }
    let team = TeamId(session.default_team());
    if session
        .teams
        .get_or_null(team)
        .is_none_or(|data| data.core().is_none())
    {
        return;
    }
    let stacks: Vec<(String, i32)> = session
        .rules
        .loadout
        .iter()
        .filter_map(|stack| stack.item.clone().map(|item| (item, stack.amount)))
        .collect();
    let item_count = registry.items().len();
    let inventory = session.teams.inventory(team, item_count);
    inventory.clear();
    for (name, amount) in stacks {
        if let Some(item) = registry.item_id(&name) {
            inventory.add(item, amount, i32::MAX);
        }
    }
}

/// `Control.playNewSector(origin, sector, reloader, params, beforePlay)`.
///
/// `reloader` is invoked at begin/end exactly like upstream; pass [`NoReloader`]
/// for a host that does not need the world-reset handshake.
#[allow(clippy::too_many_arguments)]
pub fn play_new_sector(
    session: &mut PlaySession,
    campaign: &mut Campaign,
    registry: &ContentRegistry,
    planet: PlanetId,
    sector_id: u16,
    origin: Option<SectorRef>,
    epoch: &mut RulesEpoch,
    reloader: &mut dyn super::world_reloader::WorldReloader,
) -> Vec<PlayEvent> {
    reloader.begin(session);

    session.sector = Some((planet, sector_id));
    // `World.loadSector` rules half: sector preset + planet campaign rules.
    apply_sector_preset_rules(session, campaign, registry, planet, sector_id);
    let planet_name = campaign.planet_name(planet).unwrap_or_default().to_owned();
    session.rules.sector = Some(SectorKey::format(&planet_name, sector_id));

    if let Some(sector) = campaign.sector_mut(planet, sector_id) {
        sector.info.info.origin = origin.map(|(_planet, s)| SectorKey::format(&planet_name, s));
        sector.info.info.destination = origin.map(|(_, s)| SectorKey::format(&planet_name, s));
        sector.info.info.attempts += 1;
        sector.being_played = true;
    }

    let incoming = session.rules.clone();
    let load = apply_rules_load(&mut session.rules, incoming, false, epoch);
    let mut events = vec![PlayEvent::RulesLoad {
        from_save: false,
        rules_epoch: load.rules_epoch,
    }];
    events.extend(logic_play(session));
    apply_launch_loadout(session, registry);
    // `Control.playNewSector` fires `SectorLaunchEvent` after `Logic.play`
    // (`Control.java` `playNewSector`; the port queues the typed row for the
    // host `Events` bus, GAPS #37).
    session.push_event_row(events::EventRow::Game(
        events::GameEvent::SectorLaunchEvent(events::SectorLaunchEvent {
            planet,
            sector: sector_id,
        }),
    ));
    events.push(PlayEvent::SectorLaunch {
        planet,
        sector: sector_id,
    });
    events.push(PlayEvent::NewGame);

    reloader.end(session);
    session.phase = State::Playing;
    events
}

/// `Control.playSector(origin, sector, reloader)` campaign branch selection.
///
/// The no-core/clear-save branch launches a fresh sector; the normal branch
/// loads the existing save and fires `RulesLoadEvent(fromSave=true)`. The
/// damaged-core re-derivation is a plan-06/07 world concern and is deferred
/// (the sector is relaunched fresh, matching upstream's `hadNoCore` fallback).
#[allow(clippy::too_many_arguments)]
pub fn play_sector(
    session: &mut PlaySession,
    campaign: &mut Campaign,
    registry: &ContentRegistry,
    planet: PlanetId,
    sector_id: u16,
    origin: Option<SectorRef>,
    epoch: &mut RulesEpoch,
    reloader: &mut dyn super::world_reloader::WorldReloader,
) -> Vec<PlayEvent> {
    let (has_save, has_core, clear_on_lose) = campaign
        .sector(planet, sector_id)
        .map(|sector| {
            (
                sector.has_save(),
                sector.info.info.has_core,
                sector.planet_clear_on_lose(campaign, planet),
            )
        })
        .unwrap_or((false, false, false));

    let _ = clear_on_lose;
    if !has_save || !has_core {
        return play_new_sector(
            session, campaign, registry, planet, sector_id, origin, epoch, reloader,
        );
    }

    session.reset_world();
    session.sector = Some((planet, sector_id));
    apply_sector_preset_rules(session, campaign, registry, planet, sector_id);
    let planet_name = campaign.planet_name(planet).unwrap_or_default().to_owned();
    session.rules.sector = Some(SectorKey::format(&planet_name, sector_id));
    if let Some(sector) = campaign.sector_mut(planet, sector_id) {
        sector.being_played = true;
    }
    // `SectorInfo.write()` on the resume path (`Logic` `SaveLoadEvent`): the
    // stored record's wave progress/attack state applies over the preset rules.
    if let Some(record) = campaign
        .sector(planet, sector_id)
        .map(|sector| sector.info.info.clone())
    {
        // A port-native save written before the wave-start fix may hold 0;
        // Java never writes 0 (`GameState.wave = 1`, `GameState.java:17`).
        session.wave = record.wave.max(1);
        session.rules.waves = record.waves;
        session.rules.wave_spacing = record.wave_spacing;
        session.rules.win_wave = record.win_wave;
        session.rules.attack_mode = record.attack;
    }
    let incoming = session.rules.clone();
    let load = apply_rules_load(&mut session.rules, incoming, true, epoch);
    session.phase = State::Playing;
    // `World.loadState` (save load) ends in the same `WorldLoadEvent` as a map
    // load (`World.java:220-230`); `Logic.play` is skipped on this path.
    session.push_event_row(events::EventRow::Game(events::GameEvent::WorldLoadEvent(
        events::WorldLoadEvent,
    )));
    vec![PlayEvent::RulesLoad {
        from_save: true,
        rules_epoch: load.rules_epoch,
    }]
}

/// `Logic.play()` campaign half: wave timing, stats reset, loadout, `PlayEvent`.
///
/// Called on every new play (`playMap`/`playNewSector`), never on a save load;
/// save loads keep the saved weather cooldowns (`rules.weather` is in the
/// saved rules) and the saved `wavetime`/`tick`/`stats` (T2).
pub fn logic_play(session: &mut PlaySession) -> Vec<PlayEvent> {
    session.phase = State::Playing;
    let base = if session.rules.initial_wave_spacing <= 0.0 {
        session.rules.wave_spacing * 2.0
    } else {
        session.rules.initial_wave_spacing
    };
    let multiplier = if session.is_campaign() {
        session.difficulty_wave_time_multiplier
    } else {
        1.0
    };
    session.wavetime = base * multiplier;
    session.stats.reset();
    // Map objectives load with the map (`state.rules.objectives`).
    session.objectives = MapObjectivesRuntime::from_rules(&session.rules);
    // `PlayEvent` weather reset (`Logic.java:90-99`): re-shuffled once per play,
    // including save loads, rather than only when every cooldown reads zero.
    session.weather_reset_pending = true;
    // `Logic.play` fires `PlayEvent` (`Logic.java:58-101`); queue the typed row
    // for the host `Events` bus (GAPS #37).
    session.push_event_row(events::EventRow::Sim(SimEvent::PlayEvent(
        events::PlayEvent,
    )));
    vec![PlayEvent::Play]
}

/// `Logic.runWave()` campaign difficulty scaling.
///
/// Returns the `SpawnGroup.getSpawned` index (the pre-increment `wave - 1`,
/// `WaveSpawner.java:74`) alongside the event so callers cannot invert Java's
/// order: `spawnEnemies()` runs BEFORE `state.wave++` (`Logic.java:319-321`).
pub fn run_wave_campaign(session: &mut PlaySession) -> (PlayEvent, i32) {
    let spawn_index = session.wave - 1;
    session.wave += 1;
    let multiplier = if session.is_campaign() {
        session.difficulty_wave_time_multiplier
    } else {
        1.0
    };
    session.wavetime = session.rules.wave_spacing * multiplier;
    (PlayEvent::Wave, spawn_index)
}

/// `Logic.isWaitingWave` (`Logic.java:621-624`): the countdown is held while
/// enemies are alive and either `waitEnemies` is set or the win wave has been
/// reached.
pub fn is_waiting_wave(session: &PlaySession) -> bool {
    (session.rules.wait_enemies
        || (session.wave >= session.rules.win_wave && session.rules.win_wave > 0))
        && session.enemies > 0
}

/// `Logic` WorldLoadEvent campaign half (`Logic.java:105-125`): core
/// incineration on, no infinite player resources, locked editor rules, and the
/// wave team's infinite items / build speed / factory activation delay derived
/// from the planet (`Planet.java:125-133`).
pub fn apply_world_load_campaign_rules(rules: &mut Rules, planet: &Planet) {
    rules.core_incinerates = true;
    rules.infinite_resources = false;
    rules.allow_edit_rules = false;
    rules.allow_edit_world_processors = false;
    rules.world_processor_player_link = false;
    let wave_team = rules.wave_team;
    if planet.enemy_infinite_items {
        let team = rules.team_rule_mut(wave_team);
        team.infinite_resources = true;
        team.fill_items = true;
    }
    let team = rules.team_rule_mut(wave_team);
    team.build_speed_multiplier *= planet.enemy_build_speed_multiplier;
    if planet.enemy_factory_activation_delay > 0.0 && team.unit_factory_activation_delay == 0.0 {
        team.unit_factory_activation_delay = planet.enemy_factory_activation_delay;
    }
}

/// `Logic` SaveLoadEvent rule half (`Logic.java:73-91`): core incineration,
/// game-over checks and locked editor rules on every save load.
pub fn apply_save_load_campaign_rules(rules: &mut Rules) {
    rules.core_incinerates = true;
    rules.can_game_over = true;
    rules.allow_edit_rules = false;
}

/// `SaveVersion.readRules` + `World.setSectorRules` (T5): the loaded
/// preset/generated map (or sector save) rules replace the session rules
/// wholesale, then the planet `ruleSetter`, the preset overlays and the
/// campaign load overrides are re-applied.
///
/// `has_preset` restores the preset's capture win wave on a fresh launch
/// (`SectorPreset.rules` = `winWave = captureWave`); `resumed` keeps the wave
/// record `SectorInfo.write` copied in `play_sector` and re-applies the
/// `SaveLoadEvent` overrides (`Logic.java:73-91`).
///
/// Extracted from the free `adopt_map_rules` (`mind-gdext`
/// `campaign.rs:7250-7288` at `291e30a8`; adopted by X01).
pub fn adopt_map_rules(
    session: &mut PlaySession,
    map: &Rules,
    planet: &Planet,
    registry: &ContentRegistry,
    has_preset: bool,
    preset_no_lighting: bool,
    resumed: bool,
) {
    let kept = session.rules.clone();
    let mut rules = map.clone();
    // Empty spawns fall back to the generated/default waves.
    if rules.spawns.is_empty() {
        rules.spawns = kept.spawns.clone();
    }
    rules.sector = kept.sector.clone();
    rules.loadout = kept.loadout.clone();
    planet.apply_rules(registry, &mut rules, false, true);
    if has_preset && !resumed {
        rules.win_wave = kept.win_wave;
        rules.waves = kept.waves;
        rules.attack_mode = kept.attack_mode;
    }
    if resumed {
        rules.waves = kept.waves;
        rules.wave_spacing = kept.wave_spacing;
        rules.win_wave = kept.win_wave;
        rules.attack_mode = kept.attack_mode;
    }
    // Preset `noLighting` (PAR-0059) survives the wholesale copy.
    if !kept.lighting && preset_no_lighting {
        rules.lighting = false;
    }
    apply_world_load_campaign_rules(&mut rules, planet);
    if resumed {
        apply_save_load_campaign_rules(&mut rules);
    }
    session.rules = rules;
}

/// The played sector's `sectorCaptureReplacements` (`Planet.sectorCaptureReplacements`,
/// `Planet.java`; `pump_runtime`'s capture arm): map rules applied when a
/// capture landed.
///
/// The caller keeps the `SectorCapture` event gate (`pump_runtime` 1027-1044 at
/// `291e30a8`).
///
/// Extracted from `mind-gdext` `campaign.rs` (adopted by X01).
pub fn capture_replacements(
    registry: &ContentRegistry,
    session: &PlaySession,
) -> Vec<(String, String)> {
    let planet = session.sector.map(|(planet, _)| planet);
    planet
        .and_then(|planet| registry.planet(planet))
        .map(|planet| planet.sector_capture_replacements.clone())
        .unwrap_or_default()
}

/// One campaign sector launch, decided (`Control.playSector`'s branch
/// selection, `Control.java:449-470`). The facade keeps the host reads, the
/// corrupt-save handling, the toasts and the side effects.
#[derive(Debug, Clone, PartialEq)]
pub struct SectorLaunch {
    /// A save with a live core: load it (`play_sector`).
    pub resume: bool,
    /// A lost sector's save qualified for the no-core replay
    /// (`!clearSave && spawnPosition != 0 && sectorDataMatches`; PAR-0084).
    pub lost_replay: bool,
    /// The record had an associated save.
    pub has_save: bool,
    /// The sector preset (`SectorPreset.rules` overlay input).
    pub preset: Option<crate::content::SectorId>,
    /// The preset def name (`SectorInfo.sectorDataMatches`).
    pub preset_name: Option<String>,
    /// The preset def's `noLighting` (PAR-0059).
    pub preset_no_lighting: bool,
    /// The record's packed spawn position (`SectorInfo.spawnPosition`).
    pub spawn_position: i32,
}

/// Why [`select_sector_launch`] rejected a launch (`start_sector`'s early
/// `return false`s; the facade keeps the log/toast).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchReject {
    /// The planet has no sector with that id (`start_sector`: "sector out of
    /// range").
    SectorOutOfRange,
    /// `presetRequireUnlock` and the preset is not unlocked (C-9/GAP-12).
    LockedPreset,
}

/// `Control.playSector`'s decision core (`start_sector_with_loadout` at
/// `mind-gdext` `campaign.rs:1453-1551` at `291e30a8`): validate the locked
/// preset, then pick the resume/lost-replay/fresh branch.
///
/// The caller resolves the planet name and the live-core count first; the
/// host, registry and record side effects stay in the facade (adopted by X01).
pub fn select_sector_launch(
    campaign: &Campaign,
    registry: &ContentRegistry,
    planet: PlanetId,
    sector: u16,
    no_live_core: bool,
) -> Result<SectorLaunch, LaunchReject> {
    let Some(record) = campaign.sector(planet, sector) else {
        return Err(LaunchReject::SectorOutOfRange);
    };
    let always_unlocked = record.preset_always_unlocked;
    let require_unlock = record.preset_require_unlock;
    let preset = record.preset;
    let has_save = record.has_save();
    let spawn_position = record.info.info.spawn_position;
    let had_no_core = !record.info.info.has_core;
    let preset_no_lighting = record.preset_no_lighting;
    let tech_unlocked = preset.is_none_or(|preset| {
        tech_tree::content_unlocked(registry, ContentRef::of(ContentType::Sector, preset))
    });
    if require_unlock && !always_unlocked && !tech_unlocked {
        return Err(LaunchReject::LockedPreset);
    }
    // `Control.playSector`'s branch selection (`Control.java:449-470`): the
    // normal load path is a save with a live core; a save whose sector was
    // lost (`no_live_core || had_no_core`) takes the no-core branch, which
    // replays only when `!clearSave && spawnPosition != 0 &&
    // sectorDataMatches` (PAR-0084).
    let no_core_branch = no_live_core || had_no_core;
    let clear_save = campaign
        .planet(planet)
        .is_some_and(|planet| planet.campaign_rules.clear_sector_on_lose);
    let preset_name = preset.and_then(|preset| registry.sector(preset).map(|def| def.name.clone()));
    // Deviation: the preset def carries no map dimensions, so the record's own
    // size is passed; the effective test is the preset name
    // (`SectorInfo.sectorDataMatches`, `SectorInfo.java:98-103`).
    let data_matches = record.sector_data_matches(
        preset_name.as_deref(),
        record.info.info.last_width,
        record.info.info.last_height,
    );
    let lost_replay =
        has_save && no_core_branch && !clear_save && spawn_position != 0 && data_matches;
    // Java's load branch: a save that is not in the no-core branch.
    let resume = has_save && !no_core_branch;
    Ok(SectorLaunch {
        resume,
        lost_replay,
        has_save,
        preset,
        preset_name,
        preset_no_lighting,
        spawn_position,
    })
}

/// `Control.playSector` (`Control.java:437-448`): quiet-unlock the preset (no
/// unlock event) and record the planet's last sector.
///
/// The two writes share the one settings store. `store` is the concrete
/// [`SettingsStore`] because `Campaign::set_last_sector` persists through it;
/// the unlock write wraps it in a [`tech_tree::SettingsUnlockStore`].
///
/// Extracted from `mind-gdext` `campaign.rs:1503-1520` at `291e30a8` (adopted
/// by X01).
pub fn begin_sector_launch(
    campaign: &mut Campaign,
    registry: &mut ContentRegistry,
    settings: &mut SettingsStore,
    planet: PlanetId,
    sector: u16,
) {
    let preset = campaign
        .sector(planet, sector)
        .and_then(|record| record.preset);
    let preset_name = preset.and_then(|preset| registry.sector(preset).map(|def| def.name.clone()));
    if let Some(preset) = preset
        && let Some(name) = preset_name.as_deref()
        && let Some(fields) =
            tech_tree::unlock_fields_mut(registry, ContentRef::of(ContentType::Sector, preset))
    {
        fields.quiet_unlock(name, &mut tech_tree::SettingsUnlockStore::new(settings));
    }
    if let Some(planet) = campaign.planet_mut(planet) {
        planet.set_last_sector(settings, sector);
    }
}

/// `Sector.isBeingPlayed` (`Sector.java:148-151`): a launch clears every
/// previous flag before the new sector starts (`start_sector`, 1628-1634 at
/// `291e30a8`).
///
/// H01 adds `Campaign::clear_being_played`; X01 must prefer it and C01 removes
/// this copy.
pub fn clear_being_played_for_launch(campaign: &mut Campaign) {
    for planet in campaign.planets.values_mut() {
        for sector in &mut planet.sectors {
            sector.being_played = false;
        }
    }
}

/// The live sector record for `SectorInfo.prepare` + `SectorInfo.saveInfo`
/// (`persist_sector_record_in` 4984-5056 at `291e30a8`): prepare from the live
/// core snapshot, then copy the wave/rules fields.
///
/// The caller writes the returned record into the live campaign, the facade
/// copy and the settings store; the host's core snapshot read stays in the
/// facade (finding #1 made `registry` explicit, which this signature keeps).
///
/// Extracted from `mind-gdext` `campaign.rs` (adopted by X01).
pub fn live_sector_record(
    session: &PlaySession,
    campaign: &Campaign,
    registry: &ContentRegistry,
    snapshot: Option<&CoreSnapshot>,
) -> Option<SectorInfo> {
    let (planet, sector_id) = session.sector?;
    let mut record = campaign.sector(planet, sector_id)?.info.info.clone();
    if let Some(snapshot) = snapshot {
        // `SectorInfo.prepare` `lastPresetName` (`SectorInfo.java:226-228`).
        let preset = campaign
            .sector(planet, sector_id)
            .and_then(|sector| sector.preset);
        let preset_name =
            preset.and_then(|preset| registry.sector(preset).map(|def| def.name.clone()));
        let mut prepared = SectorInfoState {
            info: record,
            ..Default::default()
        };
        prepared.prepare(snapshot, true, preset_name.as_deref());
        record = prepared.info;
    }
    record.wave = session.wave;
    record.waves = session.rules.waves;
    record.wave_spacing = session.rules.wave_spacing;
    record.win_wave = session.rules.win_wave;
    record.attack = session.rules.attack_mode;
    Some(record)
}

/// `PlanetDialog.rules` -> `Planet.applyRules(state.rules)` (`Planet.java:308-309`):
/// re-apply the played planet's campaign rules after a rules edit
/// (`set_campaign_rule`'s "re-apply if the played planet" half, 5980-6050 /
/// 6080-6130 at `291e30a8`).
///
/// The caller checks that the edited planet is the played one and mirrors the
/// session rules into the live runtime; `None` when no sector is being played
/// or the planet record is missing.
///
/// Extracted from `mind-gdext` `campaign.rs` (adopted by X01).
pub fn reapply_planet_rules(
    session: &mut PlaySession,
    campaign: &Campaign,
    registry: &ContentRegistry,
) -> Option<CampaignApply> {
    let planet = session.sector.map(|(planet, _)| planet)?;
    let record = campaign.planet(planet)?;
    Some(record.apply_rules(registry, &mut session.rules, false, true))
}

/// `Logic.checkGameState()` (campaign + non-campaign branches).
pub fn check_game_state(session: &mut PlaySession, campaign: &mut Campaign) -> Vec<PlayEvent> {
    if session.is_campaign() {
        check_game_state_campaign(session, campaign)
    } else {
        check_game_state_default(session)
    }
}

/// A `GameOverEvent` raised from `Logic.checkGameState` (`Logic.java:327-374`):
/// the event's listener records the wave the run reached
/// (`state.stats.wavesLasted = state.wave`, `Control.java:132-133`; the remote
/// `Logic.gameOver` does the same, `Logic.java:435-437`) — without it a
/// core-loss game over reports `stats.wave = 0` to `GameOverDialog`.
fn raise_game_over(session: &mut PlaySession, winner: u8) -> PlayEvent {
    session.game_over = true;
    session.stats.waves_lasted = session.wave;
    // `Logic.gameOver` fires `GameOverEvent` (`Logic.java:327-374`); queue the
    // typed row for the host `Events` bus (GAPS #37).
    session.push_event_row(events::EventRow::Game(events::GameEvent::GameOverEvent(
        events::GameOverEvent { winner },
    )));
    PlayEvent::GameOver { winner }
}

fn check_game_state_campaign(session: &mut PlaySession, campaign: &mut Campaign) -> Vec<PlayEvent> {
    let mut events = Vec::new();

    // Campaign maps have no "win" state: game over on core death.
    if session.player_core_count() == 0 && !session.game_over {
        events.push(raise_game_over(session, session.wave_team()));
    }

    // No enemy spawns -> waves disabled.
    if session.rules.waves && session.spawn_count + session.wave_core_count() as i32 <= 0 {
        session.rules.waves = false;
    }

    // Win wave reached or attack-mode enemy dead.
    let win_wave_reached = session.rules.waves
        && session.enemies == 0
        && session.rules.win_wave > 0
        && session.wave >= session.rules.win_wave
        && !session.is_spawning;
    let attack_done = session.rules.attack_mode && !session.wave_team_alive();
    if win_wave_reached || attack_done {
        if session.attack_after_waves && !session.rules.attack_mode {
            session.rules.attack_mode = true;
            session.rules.waves = false;
            // `Call.setRules` host relay.
            events.push(PlayEvent::SetRules { rules_epoch: 0 });
        } else {
            events.extend(sector_capture(session, campaign));
        }
    }
    events
}

fn check_game_state_default(session: &mut PlaySession) -> Vec<PlayEvent> {
    let mut events = Vec::new();
    if !session.rules.attack_mode && session.player_core_count() == 0 && !session.game_over {
        events.push(raise_game_over(session, session.wave_team()));
    } else if session.rules.attack_mode {
        let active = session
            .teams
            .active
            .iter()
            .filter(|team| {
                **team != TeamId(0)
                    && session
                        .teams
                        .get_or_null(**team)
                        .is_some_and(|data| data.is_alive())
            })
            .count();
        if (active <= 1
            || (!session.rules.pvp
                && session
                    .teams
                    .get_or_null(TeamId(session.default_team()))
                    .is_none_or(|data| data.core().is_none())))
            && !session.game_over
        {
            let winner = session
                .teams
                .active
                .iter()
                .find(|team| {
                    **team != TeamId(0)
                        && session
                            .teams
                            .get_or_null(**team)
                            .is_some_and(|data| data.is_alive())
                })
                .map_or(0, |team| team.0);
            events.push(raise_game_over(session, winner));
        }
    } else if !session.game_over
        && session.rules.waves
        && session.enemies == 0
        && session.rules.win_wave > 0
        && session.wave >= session.rules.win_wave
        && !session.is_spawning
    {
        events.push(raise_game_over(session, session.default_team()));
    }
    events
}

/// `Logic.sectorCapture()`: disable waves, fire capture, clear
/// markers/objectives and record the campaign capture bookkeeping
/// (`sectorsCaptured`, base coverage, last-sector victory).
pub fn sector_capture(session: &mut PlaySession, campaign: &mut Campaign) -> Vec<PlayEvent> {
    session.rules.waves = false;

    let Some((planet, sector_id)) = session.sector else {
        session.rules.attack_mode = false;
        return Vec::new();
    };
    let initial = campaign
        .sector(planet, sector_id)
        .map(|sector| !sector.info.info.was_captured)
        .unwrap_or(true);
    let last_sector = campaign
        .sector(planet, sector_id)
        .is_some_and(Sector::is_last_sector);

    if let Some(sector) = campaign.sector_mut(planet, sector_id) {
        sector.info.info.was_captured = true;
        sector.info.info.waves = false;
        sector.info.info.attack = false;
        // `Logic.sectorCapture` (`Logic.java:394-425`) does not touch
        // `isBeingPlayed`: the sector stays played until the client exits it
        // (H04 T4; T1 clears the flag on `run_exit_save`).
    }
    // `Logic`'s `SectorCaptureEvent` listener: `stats.sectorsCaptured++`.
    if let Some(stats) = campaign.stats_for_mut(planet) {
        stats.sectors_captured += 1;
    }
    // `Planet.updateBaseCoverage`: threat reflects the new frontiers.
    if let Some(planet_record) = campaign.planet_mut(planet) {
        planet_record.update_base_coverage_from_neighbors();
    }
    session.rules.attack_mode = false;
    session.rules.disable_world_processors = true;
    session.markers.clear();
    session.objectives.clear();

    // `Logic.sectorCapture` fires `SectorCaptureEvent` (`Logic.java:394-425`,
    // after the attacking flag is cleared); queue the typed row for the host
    // `Events` bus (GAPS #37).
    session.push_event_row(events::EventRow::Game(
        events::GameEvent::SectorCaptureEvent(events::SectorCaptureEvent {
            planet,
            sector: sector_id,
            initial,
        }),
    ));

    let mut events = vec![PlayEvent::SectorCapture {
        planet,
        sector: sector_id,
        initial,
    }];
    // `Control`'s last-sector victory: campaign complete on the first capture.
    if last_sector && initial {
        events.push(PlayEvent::CampaignComplete {
            planet,
            sector: sector_id,
        });
    }
    events
}

/// `Logic.updateGameOver(winner)`.
pub fn update_game_over(session: &mut PlaySession, winner: u8) -> PlayEvent {
    session.game_over = true;
    session.won = winner == session.default_team();
    session.push_event_row(events::EventRow::Game(events::GameEvent::GameOverEvent(
        events::GameOverEvent { winner },
    )));
    PlayEvent::GameOver { winner }
}

/// `Logic.gameOver(winner)`.
pub fn game_over(session: &mut PlaySession, winner: u8) -> PlayEvent {
    session.stats.waves_lasted = session.wave;
    session.won = winner == session.default_team();
    session.push_event_row(events::EventRow::Game(events::GameEvent::GameOverEvent(
        events::GameOverEvent { winner },
    )));
    PlayEvent::GameOver { winner }
}

/// `Logic.sectorLose` campaign bookkeeping (core destroyed while attacked).
pub fn sector_lose(session: &mut PlaySession, winner: u8) -> PlayEvent {
    if let Some((planet, sector)) = session.sector {
        PlayEvent::SectorLose {
            planet,
            sector,
            winner,
        }
    } else {
        PlayEvent::GameOver { winner }
    }
}

/// Campaign loss bookkeeping around [`sector_lose`] (`GameOverEvent` listener +
/// `PlanetDialog` loss path): lifetime stats, `being_played` and the
/// `clearSectorOnLose` save wipe.
pub fn apply_sector_loss(
    session: &mut PlaySession,
    campaign: &mut Campaign,
    winner: u8,
) -> Vec<PlayEvent> {
    if let Some((planet, sector_id)) = session.sector {
        // `GameOverEvent` listener: `stats.sectorsLost++`.
        if let Some(stats) = campaign.stats_for_mut(planet) {
            stats.sectors_lost += 1;
        }
        let clear_on_lose = campaign
            .planet(planet)
            .is_some_and(|planet| planet.campaign_rules.clear_sector_on_lose);
        if let Some(sector) = campaign.sector_mut(planet, sector_id) {
            sector.being_played = false;
            if clear_on_lose {
                sector.save = None;
                sector.info.info.has_core = false;
            }
        }
    }
    vec![sector_lose(session, winner)]
}

/// A reloader that does nothing (headless/unit tests).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoReloader;

impl super::world_reloader::WorldReloader for NoReloader {
    fn begin(&mut self, _session: &mut PlaySession) {}
    fn end(&mut self, _session: &mut PlaySession) {}
}

/// Extension helper: whether the campaign planet clears a sector on loss.
trait PlanetClearOnLose {
    fn planet_clear_on_lose(&self, campaign: &Campaign, planet: PlanetId) -> bool;
}

impl PlanetClearOnLose for Sector {
    fn planet_clear_on_lose(&self, campaign: &Campaign, planet: PlanetId) -> bool {
        campaign
            .planet(planet)
            .is_some_and(|record| record.campaign_rules.clear_sector_on_lose)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;
    use crate::game::test_support::{campaign, raw_registry as content};

    /// T5: `apply_world_load_campaign_rules` transcribes the campaign half of
    /// `Logic`'s WorldLoadEvent (`Logic.java:105-125`): the wave team gets
    /// `infiniteResources`/`fillItems` (`Planet.enemyInfiniteItems` default
    /// true), Serpulo factories wait 7200 ticks, Erekir builds at 0.4x.
    #[test]
    fn world_load_rules_give_wave_team_infinite_items() {
        let registry = content();
        let serpulo_def = registry.planet_by_name("serpulo").unwrap();
        let serpulo = Planet::from_def(&registry, serpulo_def, 0.0);
        let mut rules = Rules {
            wave_team: 2,
            ..Rules::default()
        };
        apply_world_load_campaign_rules(&mut rules, &serpulo);
        let team = rules.team_rule(2);
        assert!(team.infinite_resources && team.fill_items);
        assert_eq!(team.unit_factory_activation_delay, 7200.0);

        let erekir_def = registry.planet_by_name("erekir").unwrap();
        let erekir = Planet::from_def(&registry, erekir_def, 0.0);
        let mut rules = Rules {
            wave_team: 3,
            ..Rules::default()
        };
        apply_world_load_campaign_rules(&mut rules, &erekir);
        assert_eq!(rules.team_rule(3).build_speed_multiplier, 0.4);
    }

    #[test]
    fn sector_preset_rules_apply() {
        let registry = content();
        let campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        let mut session = PlaySession::new(Rules::default());
        let applied =
            apply_sector_preset_rules(&mut session, &campaign, &registry, planet, sector_id);
        assert!(applied);
        assert_eq!(session.rules.win_wave, 10, "captureWave -> winWave");
        assert!(session.rules.waves, "capture sector runs waves");
        assert!(!session.rules.attack_mode);
        assert!(session.add_starting_items, "groundZero adds starting items");
        // Planet/campaign fold applied.
        assert_eq!(session.rules.planet, "serpulo");
        assert_eq!(session.rules.env, 233, "Serpulo `Planet.defaultEnv`");
        assert_eq!(session.difficulty_wave_time_multiplier, 1.0);
    }

    #[test]
    fn apply_launch_loadout_fills_core_with_stacks() {
        use crate::io::json::content_serde::JsonItemStack;

        let registry = content();
        let planet = registry.planet_id("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        let mut session = PlaySession::new(Rules::default());
        session.sector = Some((planet, sector_id));
        session.allow_launch_loadout = true;
        session.rules.loadout = vec![
            JsonItemStack {
                item: Some("copper".to_owned()),
                amount: 300,
            },
            JsonItemStack {
                item: Some("lead".to_owned()),
                amount: 200,
            },
        ];
        let rules = session.rules.clone();
        let team = TeamId(session.default_team());
        let core = bevy_ecs::entity::Entity::from_raw_u32(77).expect("test entity");
        session.teams.register_core(core, team, &rules);
        assert!(
            session
                .teams
                .get_or_null(team)
                .is_some_and(|data| data.core().is_some()),
            "core is registered before the launch loadout"
        );

        apply_launch_loadout(&mut session, &registry);

        let items = session.teams.team_items(team).expect("team inventory");
        assert_eq!(items.get(registry.item_id("copper").unwrap()), 300);
        assert_eq!(items.get(registry.item_id("lead").unwrap()), 200);
        assert_eq!(items.total(), 500, "exactly the launch loadout");
    }

    #[test]
    fn sector_capture_stats_and_last_sector() {
        let registry = content();
        let mut campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        if let Some(sector) = campaign.sector_mut(planet, sector_id) {
            sector.preset_is_last_sector = true;
        }
        let mut session = PlaySession::new(Rules::default());
        session.sector = Some((planet, sector_id));
        session.wave = 4;
        let events = sector_capture(&mut session, &mut campaign);
        assert_eq!(
            events,
            vec![
                PlayEvent::SectorCapture {
                    planet,
                    sector: sector_id,
                    initial: true
                },
                PlayEvent::CampaignComplete {
                    planet,
                    sector: sector_id
                },
            ]
        );
        assert_eq!(
            campaign
                .stats_for(planet)
                .map(|stats| stats.sectors_captured),
            Some(1)
        );
    }

    /// T6: `checkGameState`'s core-loss branch records the wave reached
    /// (`Control.java:132-133` listener + `Logic.java:435-437` remote), so the
    /// game-over dialog's "Waves Defeated" row shows the wave instead of 0.
    #[test]
    fn core_loss_game_over_records_waves_lasted() {
        let registry = content();
        let mut campaign = campaign(&registry);
        let mut session = PlaySession::new(Rules::default());
        // No core is registered for the default team -> `playerCores == 0`.
        session.wave = 7;
        let events = check_game_state(&mut session, &mut campaign);
        assert!(
            matches!(events.as_slice(), [PlayEvent::GameOver { .. }]),
            "core loss raises one game over: {events:?}"
        );
        assert!(session.game_over);
        assert_eq!(session.stats.waves_lasted, 7);

        // A second pump must not re-raise the event (or re-touch the stat).
        session.wave = 9;
        let again = check_game_state(&mut session, &mut campaign);
        assert!(again.is_empty(), "game over is one-shot: {again:?}");
        assert_eq!(session.stats.waves_lasted, 7);
    }

    #[test]
    fn sector_loss_clears_save_when_configured() {
        let registry = content();
        let mut campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        campaign
            .planet_mut(planet)
            .expect("planet")
            .campaign_rules
            .clear_sector_on_lose = true;
        if let Some(sector) = campaign.sector_mut(planet, sector_id) {
            sector.save = Some("save".to_owned());
            sector.info.info.has_core = true;
        }
        let mut session = PlaySession::new(Rules::default());
        session.sector = Some((planet, sector_id));
        let events = apply_sector_loss(&mut session, &mut campaign, 2);
        assert_eq!(
            events,
            vec![PlayEvent::SectorLose {
                planet,
                sector: sector_id,
                winner: 2
            }]
        );
        let sector = campaign.sector(planet, sector_id).expect("sector");
        assert!(sector.save.is_none());
        assert_eq!(
            campaign.stats_for(planet).map(|stats| stats.sectors_lost),
            Some(1)
        );
    }

    #[test]
    fn play_map_event_order_and_phase() {
        let mut session = PlaySession::new(Rules::default());
        let mut epoch = RulesEpoch::new();
        let events = play_map(&mut session, Rules::default(), None, |_| false, &mut epoch);
        assert_eq!(
            events,
            vec![
                PlayEvent::RulesLoad {
                    from_save: false,
                    rules_epoch: 1
                },
                PlayEvent::Play,
                PlayEvent::NewGame
            ]
        );
        assert_eq!(session.phase, State::Playing);
        assert!(!session.is_campaign());
    }

    /// GAPS #37: `playMap` queues the `WorldLoadEvent`/`PlayEvent` rows for the
    /// host `Events` bus (`World.java:220-230`, `Logic.java:58-101`), and both
    /// round-trip through it.
    #[test]
    fn play_map_queues_world_load_and_play_event_rows() {
        let mut session = PlaySession::new(Rules::default());
        let mut epoch = RulesEpoch::new();
        let _ = play_map(&mut session, Rules::default(), None, |_| false, &mut epoch);
        let rows = session.take_event_rows();
        assert_eq!(
            rows.iter()
                .map(events::EventRow::class_name)
                .collect::<Vec<_>>(),
            vec!["WorldLoadEvent", "PlayEvent"]
        );
        assert!(session.event_rows().is_empty(), "drained");

        let mut bus = events::Events::new();
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let world_seen = seen.clone();
        bus.on_world_load(move |_| world_seen.borrow_mut().push("WorldLoadEvent"));
        let play_seen = seen.clone();
        bus.on_play(move |_| play_seen.borrow_mut().push("PlayEvent"));
        for row in rows {
            bus.dispatch_row(row);
        }
        assert_eq!(*seen.borrow(), vec!["WorldLoadEvent", "PlayEvent"]);
    }

    /// `World.loadState` (the `playSector` save branch) ends in the same
    /// `WorldLoadEvent` as a map load (`World.java:220-230`).
    #[test]
    fn play_sector_save_load_queues_world_load_event_row() {
        let registry = content();
        let mut campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        {
            let sector = campaign.sector_mut(planet, sector_id).unwrap();
            sector.save = Some("save".to_owned());
            sector.info.info.has_core = true;
        }
        let mut session = PlaySession::new(Rules::default());
        let mut epoch = RulesEpoch::new();
        let mut reloader = NoReloader;
        let play_events = play_sector(
            &mut session,
            &mut campaign,
            &registry,
            planet,
            sector_id,
            None,
            &mut epoch,
            &mut reloader,
        );
        assert!(play_events.iter().any(|event| matches!(
            event,
            PlayEvent::RulesLoad {
                from_save: true,
                ..
            }
        )));
        let rows = session.take_event_rows();
        assert_eq!(
            rows.iter()
                .map(events::EventRow::class_name)
                .collect::<Vec<_>>(),
            vec!["WorldLoadEvent"]
        );
    }

    /// GAPS #37: `Control.playNewSector` fires `SectorLaunchEvent` after the
    /// play flow's `PlayEvent`.
    #[test]
    fn play_new_sector_queues_sector_launch_event_row() {
        let registry = content();
        let mut campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        let mut session = PlaySession::new(Rules::default());
        let mut epoch = RulesEpoch::new();
        let mut reloader = NoReloader;
        play_new_sector(
            &mut session,
            &mut campaign,
            &registry,
            planet,
            sector_id,
            None,
            &mut epoch,
            &mut reloader,
        );
        let rows = session.take_event_rows();
        assert_eq!(
            rows.iter()
                .map(events::EventRow::class_name)
                .collect::<Vec<_>>(),
            vec!["PlayEvent", "SectorLaunchEvent"]
        );
        assert!(matches!(
            &rows[1],
            events::EventRow::Game(events::GameEvent::SectorLaunchEvent(event))
                if event.planet == planet && event.sector == sector_id
        ));
    }

    /// GAPS #37: `Logic.sectorCapture` queues `SectorCaptureEvent` with the
    /// upstream `initialCapture` flag.
    #[test]
    fn sector_capture_queues_sector_capture_event_row() {
        let registry = content();
        let mut campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        let mut session = PlaySession::new(Rules::default());
        session.sector = Some((planet, sector_id));

        let _ = sector_capture(&mut session, &mut campaign);
        let rows = session.take_event_rows();
        assert_eq!(rows.len(), 1, "one capture row: {rows:?}");
        assert_eq!(rows[0].class_name(), "SectorCaptureEvent");
        assert!(matches!(
            &rows[0],
            events::EventRow::Game(events::GameEvent::SectorCaptureEvent(event))
                if event.initial && event.planet == planet && event.sector == sector_id
        ));

        // A recapture is not the initial capture.
        let _ = sector_capture(&mut session, &mut campaign);
        let rows = session.take_event_rows();
        assert!(matches!(
            &rows[0],
            events::EventRow::Game(events::GameEvent::SectorCaptureEvent(event)) if !event.initial
        ));
    }

    /// GAPS #37: `Logic.gameOver`/`Logic.checkGameState` queue the
    /// `GameOverEvent` row with the winning team.
    #[test]
    fn game_over_queues_game_over_event_row() {
        let mut session = PlaySession::new(Rules::default());
        session.wave = 6;
        let event = game_over(&mut session, 2);
        assert_eq!(event, PlayEvent::GameOver { winner: 2 });
        let rows = session.take_event_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].class_name(), "GameOverEvent");
        assert!(matches!(
            rows[0],
            events::EventRow::Game(events::GameEvent::GameOverEvent(event)) if event.winner == 2
        ));
    }

    /// `Logic.checkGameState`'s core-loss game over is the other `GameOverEvent`
    /// fire site.
    #[test]
    fn check_game_state_queues_game_over_event_row() {
        let registry = content();
        let mut campaign = campaign(&registry);
        let mut session = PlaySession::new(Rules::default());
        session.wave = 3;
        let play_events = check_game_state(&mut session, &mut campaign);
        assert!(
            matches!(play_events.as_slice(), [PlayEvent::GameOver { .. }]),
            "core loss raises one game over: {play_events:?}"
        );
        let rows = session.take_event_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].class_name(), "GameOverEvent");
    }

    #[test]
    fn sector_capture_flags() {
        let registry = content();
        let mut campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        // Seed a sector with a core and a marker/objective.
        {
            let sector = campaign.sector_mut(planet, sector_id).unwrap();
            sector.save = Some("save".to_owned());
            sector.info.info.has_core = true;
        }
        let mut session = PlaySession::new(Rules::default());
        session.sector = Some((planet, sector_id));
        session.markers.add(
            1,
            crate::io::json::objectives::ObjectiveMarker::Point(
                crate::io::json::objectives::PointMarker {
                    world: 1,
                    minimap: -1,
                    light: -1,
                    ..Default::default()
                },
            ),
        );
        session
            .objectives
            .add([crate::io::json::objectives::MapObjective::DestroyUnits(
                crate::io::json::objectives::DestroyUnitsObjective {
                    count: 1,
                    ..Default::default()
                },
            )]);

        let events = sector_capture(&mut session, &mut campaign);
        assert_eq!(
            events,
            vec![PlayEvent::SectorCapture {
                planet,
                sector: sector_id,
                initial: true
            }]
        );
        assert!(!session.rules.waves);
        assert!(!session.rules.attack_mode);
        assert!(session.rules.disable_world_processors);
        assert!(session.markers.size() == 0);
        assert!(session.objectives.is_empty());
        assert!(
            campaign
                .sector(planet, sector_id)
                .unwrap()
                .info
                .info
                .was_captured
        );
    }

    #[test]
    fn game_over_winner_campaign() {
        let registry = content();
        let mut campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        let mut session = PlaySession::new(Rules::default());
        session.sector = Some((planet, sector_id));
        // No player core -> immediate game over for the wave team.
        let events = check_game_state(&mut session, &mut campaign);
        assert_eq!(
            events,
            vec![PlayEvent::GameOver {
                winner: session.wave_team()
            }]
        );
        assert!(session.game_over);
    }

    #[test]
    fn set_rules_guard_rejects_campaign_edit() {
        use crate::game::rules_event::{SetRulesError, apply_set_rules};
        let mut session = PlaySession::new(Rules::default());
        session.sector = Some((PlanetId::new(0), 0));
        let mut epoch = RulesEpoch::new();
        let is_campaign = session.is_campaign();
        let result = apply_set_rules(
            &mut session.rules,
            Rules::default(),
            is_campaign,
            Some(0),
            &mut epoch,
        );
        assert!(matches!(result, Err(SetRulesError::CampaignReadOnly)));

        // Non-campaign edit is accepted and bumps the epoch.
        let mut open = PlaySession::new(Rules::default());
        let open_is_campaign = open.is_campaign();
        let new_epoch = apply_set_rules(
            &mut open.rules,
            Rules::default(),
            open_is_campaign,
            Some(0),
            &mut epoch,
        )
        .unwrap();
        assert_eq!(new_epoch, 1);
    }

    #[test]
    fn world_reloader_and_run_wave() {
        use super::super::world_reloader::HostReloader;
        let registry = content();
        let mut campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        let mut session = PlaySession::new(Rules::default());
        let mut epoch = RulesEpoch::new();
        let mut reloader = HostReloader::default();
        let events = play_new_sector(
            &mut session,
            &mut campaign,
            &registry,
            planet,
            sector_id,
            None,
            &mut epoch,
            &mut reloader,
        );
        assert!(reloader.began);
        assert!(reloader.was_server);
        assert!(events.contains(&PlayEvent::SectorLaunch {
            planet,
            sector: sector_id
        }));
        assert_eq!(session.phase, State::Playing);

        session.difficulty_wave_time_multiplier = 2.0;
        let (event, spawn_index) = run_wave_campaign(&mut session);
        assert_eq!(event, PlayEvent::Wave);
        assert_eq!(spawn_index, 0, "wave 1 spawns getSpawned(0)");
        assert_eq!(session.wave, 2);
        assert_eq!(session.wavetime, session.rules.wave_spacing * 2.0);
    }

    /// H04 T3: `Control.playNewSector` (`Control.java:573-578`) records the
    /// launcher sector as `origin`/`destination` (`SectorKey::format`).
    #[test]
    fn launch_sets_origin_and_destination() {
        let registry = content();
        let mut campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").unwrap();
        let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
        let mut session = PlaySession::new(Rules::default());
        let mut epoch = RulesEpoch::new();
        let mut reloader = NoReloader;
        play_new_sector(
            &mut session,
            &mut campaign,
            &registry,
            planet,
            sector_id,
            Some((planet, 15)),
            &mut epoch,
            &mut reloader,
        );
        let record = campaign.sector(planet, sector_id).unwrap();
        assert_eq!(
            record.info.info.origin.as_ref().map(|key| key.0.as_str()),
            Some("serpulo-15")
        );
        assert_eq!(
            record
                .info
                .info
                .destination
                .as_ref()
                .map(|key| key.0.as_str()),
            Some("serpulo-15")
        );
    }

    /// T2b: a fresh session starts at Java's wave 1 (`GameState.java:17`) and
    /// `logic.reset()` restores it (`Logic.java:308`).
    #[test]
    fn fresh_session_starts_at_wave_one() {
        let mut session = PlaySession::new(Rules::default());
        assert_eq!(session.wave, 1, "fresh session is wave 1");
        session.wave = 5;
        session.reset_world();
        assert_eq!(session.wave, 1, "logic.reset restores wave 1");
    }

    /// T2b: `run_wave_campaign` returns the pre-increment `getSpawned` index
    /// (`WaveSpawner.java:74`) so callers cannot invert Java's order.
    #[test]
    fn run_wave_returns_pre_increment_index() {
        let mut session = PlaySession::new(Rules::default());
        let (event, spawn_index) = run_wave_campaign(&mut session);
        assert_eq!(event, PlayEvent::Wave);
        assert_eq!(spawn_index, 0, "wave 1 -> getSpawned(0)");
        assert_eq!(session.wave, 2);
        session.wave = 5;
        let (_, spawn_index) = run_wave_campaign(&mut session);
        assert_eq!(spawn_index, 4, "wave 5 -> getSpawned(4)");
        assert_eq!(session.wave, 6);
    }

    // --- campaign API (W2-H02) ---

    #[test]
    fn capture_replacements_read_the_played_planet() {
        let registry = content();
        let planet = registry.planet_id("serpulo").unwrap();
        let mut session = PlaySession::new(Rules::default());
        assert!(capture_replacements(&registry, &session).is_empty());
        session.sector = Some((planet, 0));
        assert_eq!(
            capture_replacements(&registry, &session),
            registry
                .planet(planet)
                .unwrap()
                .sector_capture_replacements
                .clone()
        );
    }

    /// PAR-0037/PAR-0030 + T5: a preset map's rules replace `state.rules`
    /// wholesale in Java (`FileMapGenerator.generate` -> `SaveIO.load(map.file)`
    /// -> `SaveVersion.readRules`), so the adopted rules must carry the
    /// objective graph, its flags and the `waveTimer` gate into the session or
    /// the tutorial launches as a wave sector.
    #[test]
    fn adopt_map_rules_carries_objectives_and_wave_timer() {
        let registry = content();
        let objectives = crate::io::json::objectives::MapObjectives::from_json(
            r#"[{"class":"Item","amount":15,"markers":[{"class":"Text","text":"@gz.mine"}]}]"#,
        )
        .expect("fixture objectives");
        let map = Rules {
            wave_timer: false,
            wave_sending: false,
            objectives,
            objective_flags: std::iter::once("spawnWave".to_owned()).collect(),
            ..Rules::default()
        };
        let planet_def = registry.planet_by_name("serpulo").expect("serpulo");
        let planet = Planet::from_def(&registry, planet_def, 0.0);
        let mut session = PlaySession::new(Rules::default());
        adopt_map_rules(&mut session, &map, &planet, &registry, true, false, false);
        assert!(!session.rules.wave_timer, "map waveTimer:false must win");
        assert!(
            !session.rules.wave_sending,
            "map waveSending:false must win (canSkipWave gate - PAR-0030)"
        );
        assert_eq!(session.rules.objectives.len(), 1);
        assert!(session.rules.objective_flags.contains("spawnWave"));
        // The serpulo ruleSetter re-applied after the wholesale copy.
        assert_eq!(session.rules.wave_team, 2);
        assert!(session.rules.core_destroy_clear);
    }

    /// `SaveSlot.load` meta half: rules, wave, wave timer and stats replace the
    /// live session's and the match becomes playing.
    #[test]
    fn apply_save_meta_replaces_rules_wave_and_stats() {
        let mut session = PlaySession::new(Rules::default());
        let mut tags = crate::io::StringMap::new();
        tags.insert("wavetime".to_owned(), "42.5".to_owned());
        tags.insert("stats".to_owned(), r#"{"wavesLasted":6}"#.to_owned());
        let meta = SaveMeta {
            version: 1,
            timestamp: 0,
            time_played: 0,
            build: 0,
            map_name: String::new(),
            wave: 7,
            rules_json: r#"{"waveTeam":9,"waveSpacing":12}"#.to_owned(),
            tags,
            mods: Vec::new(),
        };
        session.apply_save_meta(&meta);
        assert_eq!(session.rules.wave_team, 9);
        assert_eq!(session.rules.wave_spacing, 12.0);
        assert_eq!(session.wave, 7);
        assert_eq!(session.wavetime, 42.5);
        assert_eq!(session.stats.waves_lasted, 6);
        assert_eq!(session.phase, State::Playing);
    }

    #[test]
    fn select_sector_launch_rejects_unknown_and_locked_sectors() {
        let mut registry = content();
        let campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").expect("serpulo");
        assert_eq!(
            select_sector_launch(&campaign, &registry, planet, u16::MAX, false),
            Err(LaunchReject::SectorOutOfRange)
        );
        // `groundZero` is always unlocked, so a fresh record launches.
        let zero = registry.sector_by_name("groundZero").expect("groundZero");
        let launch = select_sector_launch(&campaign, &registry, zero.planet, zero.sector, false)
            .expect("groundZero launches");
        assert!(!launch.resume && !launch.lost_replay && !launch.has_save);
        assert_eq!(launch.preset, Some(zero.id));
        assert_eq!(launch.preset_name.as_deref(), Some("groundZero"));
        // A locked preset is rejected while `presetRequireUnlock` holds.
        let locked = campaign
            .planet(planet)
            .expect("serpulo record")
            .sectors
            .iter()
            .find(|sector| {
                sector.preset.is_some()
                    && sector.preset_require_unlock
                    && !sector.preset_always_unlocked
            })
            .map(|sector| sector.id)
            .expect("a locked preset sector");
        assert_eq!(
            select_sector_launch(&campaign, &registry, planet, locked, false),
            Err(LaunchReject::LockedPreset)
        );
        // Unlocking the preset lifts the gate.
        let preset = campaign
            .sector(planet, locked)
            .and_then(|sector| sector.preset)
            .expect("preset");
        let name = registry
            .sector(preset)
            .map(|def| def.name.clone())
            .expect("preset def");
        let mut store = crate::content::MemoryUnlockStore::new();
        tech_tree::unlock_fields_mut(&mut registry, ContentRef::of(ContentType::Sector, preset))
            .expect("preset unlock fields")
            .unlock(&name, &mut store);
        let launch = select_sector_launch(&campaign, &registry, planet, locked, false)
            .expect("the unlocked preset launches");
        assert_eq!(launch.preset, Some(preset));
    }

    #[test]
    fn select_sector_launch_picks_the_resume_and_lost_replay_branches() {
        let registry = content();
        let mut campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").expect("serpulo");
        let sector_id = registry.sector_by_name("groundZero").expect("g0").sector;
        {
            let sector = campaign.sector_mut(planet, sector_id).expect("sector");
            sector.save = Some("sector-test".to_owned());
            sector.info.info.has_core = true;
        }
        // A live core resumes the save.
        let launch =
            select_sector_launch(&campaign, &registry, planet, sector_id, false).expect("resume");
        assert!(launch.resume && launch.has_save && !launch.lost_replay);
        // No live core plus a matching preset name and spawn position replays
        // the lost sector (PAR-0084).
        {
            let sector = campaign.sector_mut(planet, sector_id).expect("sector");
            sector.info.info.spawn_position = 1;
            sector.info.info.last_preset_name = sector.preset_name.clone();
        }
        let launch = select_sector_launch(&campaign, &registry, planet, sector_id, true)
            .expect("lost replay");
        assert!(launch.lost_replay && !launch.resume);
        assert_eq!(launch.spawn_position, 1);
        // `clearSectorOnLose` suppresses the replay.
        campaign
            .planet_mut(planet)
            .expect("serpulo record")
            .campaign_rules
            .clear_sector_on_lose = true;
        let launch =
            select_sector_launch(&campaign, &registry, planet, sector_id, true).expect("no replay");
        assert!(!launch.lost_replay && !launch.resume);
    }

    #[test]
    fn begin_sector_launch_unlocks_the_preset_and_clears_the_played_flags() {
        let mut registry = content();
        let mut campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").expect("serpulo");
        let sector_id = registry.sector_by_name("groundZero").expect("g0").sector;
        // A previous sector still flagged as played.
        campaign
            .sector_mut(planet, sector_id)
            .expect("sector")
            .being_played = true;
        clear_being_played_for_launch(&mut campaign);
        assert!(
            campaign
                .planet(planet)
                .expect("serpulo record")
                .sectors
                .iter()
                .all(|sector| !sector.being_played)
        );
        // A locked preset is quiet-unlocked (no unlock event) and recorded.
        let locked = campaign
            .planet(planet)
            .expect("serpulo record")
            .sectors
            .iter()
            .find(|sector| {
                sector.preset.is_some()
                    && sector.preset_require_unlock
                    && !sector.preset_always_unlocked
            })
            .map(|sector| sector.id)
            .expect("a locked preset sector");
        let preset = campaign
            .sector(planet, locked)
            .and_then(|sector| sector.preset)
            .expect("preset");
        let mut settings = SettingsStore::default();
        begin_sector_launch(&mut campaign, &mut registry, &mut settings, planet, locked);
        assert!(tech_tree::content_unlocked(
            &registry,
            ContentRef::of(ContentType::Sector, preset)
        ));
        assert_eq!(
            campaign.planet(planet).expect("serpulo record").last_sector,
            locked
        );
    }

    #[test]
    fn live_sector_record_copies_the_live_wave_and_rules() {
        let registry = content();
        let campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").expect("serpulo");
        let sector_id = registry.sector_by_name("groundZero").expect("g0").sector;
        let mut session = PlaySession::new(Rules::default());
        assert_eq!(
            live_sector_record(&session, &campaign, &registry, None),
            None,
            "no played sector -> no record"
        );
        session.sector = Some((planet, sector_id));
        session.wave = 12;
        session.rules.waves = true;
        session.rules.wave_spacing = 45.0;
        session.rules.win_wave = 30;
        session.rules.attack_mode = true;
        let record = live_sector_record(&session, &campaign, &registry, None).expect("record");
        assert_eq!(record.wave, 12);
        assert!(record.waves);
        assert_eq!(record.wave_spacing, 45.0);
        assert_eq!(record.win_wave, 30);
        assert!(record.attack);
    }

    #[test]
    fn reapply_planet_rules_applies_the_played_planets_rules() {
        let registry = content();
        let campaign = campaign(&registry);
        let planet = campaign.planet_id_by_name("serpulo").expect("serpulo");
        let sector_id = registry.sector_by_name("groundZero").expect("g0").sector;
        let mut session = PlaySession::new(Rules::default());
        assert!(
            reapply_planet_rules(&mut session, &campaign, &registry).is_none(),
            "no played sector -> nothing re-applied"
        );
        session.sector = Some((planet, sector_id));
        let applied = reapply_planet_rules(&mut session, &campaign, &registry)
            .expect("the played planet re-applies");
        // `Planet.applyRules` writes the planet identity and default env.
        assert_eq!(session.rules.planet, "serpulo");
        let _ = applied;
    }
}
