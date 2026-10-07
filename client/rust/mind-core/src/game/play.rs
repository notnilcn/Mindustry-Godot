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

use super::map_markers::MapMarkers;
use super::map_objectives::MapObjectivesRuntime;
use super::rules::Rules;
use super::rules_event::{RulesEpoch, apply_rules_load};
use super::sector::Sector;
use super::teams::Teams;
use super::universe::Campaign;
use crate::content::{ContentRegistry, PlanetId};
use crate::ecs::TeamId;
use crate::game::State;
use crate::io::json::content_serde::SectorKey;
use crate::io::json::rules::GameStats;

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
    /// Whether the player core loadout is applied at launch.
    pub allow_launch_loadout: bool,
    /// Whether the sector preset adds starting items.
    pub add_starting_items: bool,
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
            wave: 0,
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
            allow_launch_loadout: false,
            add_starting_items: false,
        }
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
        self.wave = 0;
        self.wavetime = 0.0;
        self.game_over = false;
        self.after_game_over = false;
        self.won = false;
        self.enemies = 0;
        self.spawn_count = 0;
        self.is_spawning = false;
        self.sector = None;
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
    events.extend(logic_play(session));
    events.push(PlayEvent::NewGame);
    events
}

/// `World.setSectorRules` rules half (`World.java:266,292,299-330`): resolve the
/// sector preset (`rules.winWave = preset.captureWave`, attack mode when there is
/// no capture wave and an enemy base) and fold the planet's campaign rules.
///
/// `Control.playNewSector` calls `world.loadSector` before `logic.play`, so a
/// fresh launch must start from neutral wave/attack state and then re-derive it
/// from the sector that is being played.
fn apply_sector_preset_rules(
    session: &mut PlaySession,
    campaign: &Campaign,
    registry: &ContentRegistry,
    planet: PlanetId,
    sector_id: u16,
) {
    session.rules.win_wave = 0;
    session.rules.waves = false;
    session.rules.attack_mode = false;

    let Some(planet_record) = campaign.planet(planet) else {
        return;
    };
    if let Some(sector) = planet_record.sector(sector_id)
        && let Some(preset_id) = sector.preset
        && let Some(preset) = registry.sector(preset_id)
    {
        // `SectorPreset.rules`: the capture wave is the win wave.
        session.rules.win_wave = preset.capture_wave;
        let attack = preset.capture_wave <= 0 && sector.has_enemy_base();
        session.rules.attack_mode = attack;
        session.rules.waves = !attack;
        if session.rules.win_wave <= 0 && !attack && planet_record.allow_waves {
            // `SectorInfo.write`: infinite waves get a default win wave.
            session.rules.win_wave = 30;
        }
    }
    // `Planet.applyRules(rules, customGame = false)`.
    planet_record.apply_rules(registry, &mut session.rules, false, false);
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
    let planet_name = campaign
        .planet(planet)
        .map(|planet| planet.name.clone())
        .unwrap_or_default();
    session.rules.sector = Some(SectorKey::format(&planet_name, sector_id));

    if let Some(sector) = campaign.sector_mut(planet, sector_id) {
        sector.info.info.origin = origin.map(|(_planet, s)| SectorKey::format(&planet_name, s));
        sector.info.info.destination = origin.map(|(_, s)| SectorKey::format(&planet_name, s));
        sector.info.info.attempts += 1;
    }

    let incoming = session.rules.clone();
    let load = apply_rules_load(&mut session.rules, incoming, false, epoch);
    let mut events = vec![PlayEvent::RulesLoad {
        from_save: false,
        rules_epoch: load.rules_epoch,
    }];
    events.extend(logic_play(session));
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
    let planet_name = campaign
        .planet(planet)
        .map(|planet| planet.name.clone())
        .unwrap_or_default();
    session.rules.sector = Some(SectorKey::format(&planet_name, sector_id));
    let incoming = session.rules.clone();
    let load = apply_rules_load(&mut session.rules, incoming, true, epoch);
    session.phase = State::Playing;
    vec![PlayEvent::RulesLoad {
        from_save: true,
        rules_epoch: load.rules_epoch,
    }]
}

/// `Logic.play()` campaign half: wave timing, stats reset, loadout, `PlayEvent`.
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

    // Starting loadout (`CoreBuild.items.clear` + capped add). Core inventories
    // are owned by plan 08; the deterministic intent is recorded here for the
    // ECS adapter to apply.
    if !session.is_campaign() || !session.allow_launch_loadout || session.add_starting_items {
        // `session.rules.loadout` is applied by the plan-08 core adapter.
    }
    vec![PlayEvent::Play]
}

/// `Logic.runWave()` campaign difficulty scaling.
pub fn run_wave_campaign(session: &mut PlaySession) -> PlayEvent {
    session.wave += 1;
    let multiplier = if session.is_campaign() {
        session.difficulty_wave_time_multiplier
    } else {
        1.0
    };
    session.wavetime = session.rules.wave_spacing * multiplier;
    PlayEvent::Wave
}

/// `Logic.checkGameState()` (campaign + non-campaign branches).
pub fn check_game_state(session: &mut PlaySession, campaign: &mut Campaign) -> Vec<PlayEvent> {
    if session.is_campaign() {
        check_game_state_campaign(session, campaign)
    } else {
        check_game_state_default(session)
    }
}

fn check_game_state_campaign(session: &mut PlaySession, campaign: &mut Campaign) -> Vec<PlayEvent> {
    let mut events = Vec::new();

    // Campaign maps have no "win" state: game over on core death.
    if session.player_core_count() == 0 && !session.game_over {
        session.game_over = true;
        events.push(PlayEvent::GameOver {
            winner: session.wave_team(),
        });
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
        session.game_over = true;
        events.push(PlayEvent::GameOver {
            winner: session.wave_team(),
        });
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
            events.push(PlayEvent::GameOver { winner });
            session.game_over = true;
        }
    } else if !session.game_over
        && session.rules.waves
        && session.enemies == 0
        && session.rules.win_wave > 0
        && session.wave >= session.rules.win_wave
        && !session.is_spawning
    {
        session.game_over = true;
        events.push(PlayEvent::GameOver {
            winner: session.default_team(),
        });
    }
    events
}

/// `Logic.sectorCapture()`: disable waves, fire capture, clear markers/objectives.
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

    if let Some(sector) = campaign.sector_mut(planet, sector_id) {
        sector.info.info.was_captured = true;
    }
    session.rules.attack_mode = false;
    session.rules.disable_world_processors = true;
    session.markers.clear();
    session.objectives.clear();

    vec![PlayEvent::SectorCapture {
        planet,
        sector: sector_id,
        initial,
    }]
}

/// `Logic.updateGameOver(winner)`.
pub fn update_game_over(session: &mut PlaySession, winner: u8) -> PlayEvent {
    session.game_over = true;
    session.won = winner == session.default_team();
    PlayEvent::GameOver { winner }
}

/// `Logic.gameOver(winner)`.
pub fn game_over(session: &mut PlaySession, winner: u8) -> PlayEvent {
    session.stats.waves_lasted = session.wave;
    session.won = winner == session.default_team();
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
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::game::planet::EmptyNeighborhood;

    fn content() -> ContentRegistry {
        let bundle = MemoryBundle::new();
        let store = MemoryUnlockStore::new();
        create_base_content(&bundle, &store, true).expect("content boot")
    }

    fn campaign(registry: &ContentRegistry) -> Campaign {
        Campaign::from_registry(registry, &EmptyNeighborhood)
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
        let event = run_wave_campaign(&mut session);
        assert_eq!(event, PlayEvent::Wave);
        assert_eq!(session.wave, 1);
        assert_eq!(session.wavetime, session.rules.wave_spacing * 2.0);
    }
}
