// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live campaign runtime: the `CampaignRuntime` ECS resource plus the
//! `TickSet::Campaign` / `TickSet::Objectives` / `TickSet::GameStateCheck`
//! systems.
//!
//! Ported from the campaign half of `core/src/mindustry/core/Logic.java`
//! (`update`/`runWave`/`checkGameState`), `core/src/mindustry/game/Universe.java`
//! (`update` clock, `updateGlobal`) and `mindustry/game/MapObjectives.java`
//! (`state.objectives.update()`). The resource hosts the live [`PlaySession`]
//! and [`Campaign`] while a sector is loaded; the systems are no-ops when it is
//! absent, so P0 `Sim` checksums and goldens are unchanged.

use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

use crate::content::PlanetId;
use crate::game::play::{PlayEvent, PlaySession};
use crate::game::rules::Rules;
use crate::game::service::ServiceTracker;
use crate::game::universe::Campaign;
use crate::math::ArcRand;

/// Sim ticks per second (fixed rate).
const TICKS_PER_SECOND: f32 = 60.0;
/// `Universe.update` global-position refresh period in ticks (one second).
const GLOBAL_UPDATE_TICKS: u64 = 60;
/// `GameService.registerEvents`'s `Timer.schedule(checkUpdate, 2f, 2f)` period
/// in ticks (GAPS #68).
const SERVICE_CHECK_TICKS: u64 = 120;

/// The campaign match runtime installed into the sim ECS world while a sector
/// is loaded.
#[derive(Resource)]
pub struct CampaignRuntime {
    /// Live match/session state (`GameState` plan-12 half).
    pub session: PlaySession,
    /// Live campaign container (planets/sectors/stats).
    pub campaign: Campaign,
    /// Ordered events produced by the tick systems for the host to consume.
    pub events: Vec<PlayEvent>,
    /// Completed fixed ticks since the sector loaded.
    pub ticks: u64,
    /// The campaign clock reached a turn boundary (`Universe.update`).
    pub turn_due: bool,
    /// Running as a network client (`net.client()`): server-only systems skip.
    pub is_client: bool,
    /// Unlocked content names (map objective `Research`/`Produce` predicates).
    pub unlocked_content: std::collections::HashSet<String>,
    /// Deterministic team-AI randomness (`BaseBuilderAI` placement scatter).
    ///
    /// Upstream draws from the global `Mathf.random`; the live port keeps a
    /// dedicated stream so a scenario replays byte-for-byte (PAR-0139).
    pub ai_rng: ArcRand,
    /// Sector capture converted the wave team to derelict on the next
    /// `Sim::tick` (`TeamData.destroyToDerelict`, Teams.java:331-362): a
    /// system cannot mutate the world mid-schedule, so
    /// `game_state_tick_system` queues the team here.
    pub pending_derelict_team: Option<u8>,
    /// `coreDestroyClear` posts (`Logic` `BlockDestroyEvent`,
    /// Logic.java:182-186): `(team, x, y, range)` drained by
    /// `Sim::apply_campaign_world_requests` into `TeamData.timeDestroy`.
    pub pending_time_destroy: Vec<(u8, f32, f32, f32)>,
    /// A mid-match rules change must re-derive the world (limit area, build
    /// rules): set by G06's `setrule`, the `Call.setRules` relay and the
    /// universe invasion; `Sim::tick` drains it (T5).
    pub rules_world_dirty: bool,
    /// Newly produced content queued for the host to unlock through the tech
    /// tree (`UnlockRef`; drained by `MindCampaign.take_unlocked_content`).
    pub pending_unlocks: Vec<UnlockRef>,
    /// Destination sector records queued by `launch_payload_system` for a host
    /// `save_info` flush (legacy launch pads; H04 T6).
    pub pending_info_saves: Vec<(PlanetId, u16)>,
    /// Ticks left before an abandoned played sector's player cores die
    /// (`PlanetDialog.abandonSectorConfirm`: `Time.runTask(7f, ...)`,
    /// PlanetDialog.java:1156-1161); drained by
    /// `Sim::apply_campaign_world_requests`.
    pub pending_abandon_kill: Option<u32>,
    /// Local achievement/stat tracking (`Vars.service` + `GameService`; GAPS
    /// #68). The host loads/saves the service through
    /// [`ServiceTracker::load`]/[`ServiceTracker::save`] and may replace it
    /// after boot; the campaign systems drive the gameplay hooks.
    pub service: ServiceTracker,
}

/// `Time.runTask(7f, ...)` in `abandonSectorConfirm` (PlanetDialog.java:1156).
pub const ABANDON_KILL_DELAY_TICKS: u32 = 7;

impl CampaignRuntime {
    /// Wraps the launched session/campaign.
    pub fn new(session: PlaySession, campaign: Campaign) -> Self {
        Self {
            session,
            campaign,
            events: Vec::new(),
            ticks: 0,
            turn_due: false,
            is_client: false,
            unlocked_content: std::collections::HashSet::new(),
            ai_rng: ArcRand::new(0x5EED_0139),
            pending_derelict_team: None,
            pending_time_destroy: Vec::new(),
            rules_world_dirty: false,
            pending_unlocks: Vec::new(),
            pending_info_saves: Vec::new(),
            pending_abandon_kill: None,
            service: ServiceTracker::new(),
        }
    }

    /// Installs a pre-loaded service (host boot: settings -> [`ServiceTracker`]).
    pub fn with_service(mut self, service: ServiceTracker) -> Self {
        self.service = service;
        self
    }

    /// `PlanetDialog.abandonSectorConfirm`, played-sector branch
    /// (PlanetDialog.java:1153-1161): after the dialog hides, `Time.runTask(7f)`
    /// kills every core of `player.team()` "to force game over in a more
    /// dramatic fashion"; the campaign `checkGameState` (Logic.java:329-334)
    /// then fires the `GameOverEvent`. The record is not cleared here: the
    /// loss path owns it.
    pub fn abandon_played_sector(&mut self) {
        if self.session.is_campaign() {
            self.pending_abandon_kill = Some(ABANDON_KILL_DELAY_TICKS);
        }
    }

    /// Replaces the unlocked-content cache (host-side registry projection).
    pub fn set_unlocked_content(&mut self, names: impl IntoIterator<Item = String>) {
        self.unlocked_content = names.into_iter().collect();
    }

    /// Current match rules.
    pub fn rules(&self) -> &Rules {
        &self.session.rules
    }

    /// Drains the queued events (host-side bookkeeping).
    pub fn take_events(&mut self) -> Vec<PlayEvent> {
        std::mem::take(&mut self.events)
    }
}

/// Live match rules from a world, or `None` in test/harness worlds without a
/// [`CampaignRuntime`] (combat multipliers are 1.0 there).
pub fn rules_of(world: &World) -> Option<&Rules> {
    world
        .get_resource::<CampaignRuntime>()
        .map(|runtime| &runtime.session.rules)
}

mod game_state;
mod hooks;
mod objectives;
mod session;
mod team_ai;
mod tick;
mod weather;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

pub use game_state::{current_sector_captured, game_state_tick_system, on_captured_sector};
pub use hooks::{
    CampaignCoreHooks, CampaignCoreQueues, CampaignHookQueues, UnlockRef,
    install_campaign_core_hooks, reset_campaign_core_hooks,
};
pub use objectives::{ObjectiveInputState, objectives_tick_system};
pub use session::{SessionSync, apply_played_invasions, run_wave_now, sync_session_with_sim};
pub use team_ai::{
    AiPlaceRequest, BaseCatalog, CampaignAiQueues, install_campaign_ai_queues, team_ai_system,
};
pub use tick::campaign_tick_system;
pub use weather::{weather_system, weather_water_attr};
