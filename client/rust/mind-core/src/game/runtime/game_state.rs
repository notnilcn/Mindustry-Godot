// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TickSet::GameStateCheck`: `Logic.checkGameState` plus the campaign
//! sector-capture/loss bookkeeping.
//! Upstream: core/src/mindustry/core/Logic.java (`checkGameState`)

use super::*;
use crate::game::State;
use crate::game::play::{apply_sector_loss, check_game_state, sector_capture};

/// `TickSet::GameStateCheck`: `Logic.checkGameState` + campaign loss
/// bookkeeping.
///
/// `Logic.update` runs the check only when `state.rules.canGameOver`
/// (`Logic.java:521`).
pub fn game_state_tick_system(world: &mut World) {
    let Some(mut runtime) = world.remove_resource::<CampaignRuntime>() else {
        return;
    };
    if runtime.session.phase != State::Playing || runtime.is_client {
        world.insert_resource(runtime);
        return;
    }
    if !runtime.session.rules.can_game_over {
        world.insert_resource(runtime);
        return;
    }
    // `SectorCaptureEvent` listener inputs: Java fires the event inside
    // `Logic.sectorCapture` *before* `rules.attackMode = false`, so the
    // listener sees the attacking flag and the pre-capture loss counter.
    let was_attack_mode = runtime.session.rules.attack_mode;
    let buildings_destroyed = runtime.session.stats.buildings_destroyed;
    let events = check_game_state(&mut runtime.session, &mut runtime.campaign);
    for event in events {
        // `GameService` listeners (GAPS #68): sector capture, last-sector
        // victory and the PvP win approximation.
        match &event {
            PlayEvent::SectorCapture { planet, .. } => {
                on_captured_sector(&mut runtime, *planet, was_attack_mode, buildings_destroyed)
            }
            PlayEvent::CampaignComplete { planet, .. } => runtime
                .service
                .on_campaign_complete(&runtime.campaign, *planet),
            PlayEvent::GameOver { winner } => {
                runtime.service.on_game_over(&runtime.session, *winner)
            }
            _ => {}
        }
        if matches!(event, PlayEvent::GameOver { .. }) && runtime.session.is_campaign() {
            let winner = match event {
                PlayEvent::GameOver { winner } => winner,
                _ => 0,
            };
            runtime.events.extend(apply_sector_loss(
                &mut runtime.session,
                &mut runtime.campaign,
                winner,
            ));
        } else {
            // Sector-capture bookkeeping (the service hook and the derelict
            // wave-team queue) was already applied by [`on_captured_sector`].
            runtime.events.push(event);
        }
    }
    world.insert_resource(runtime);
}

/// `SectorCaptureEvent` bookkeeping shared by [`game_state_tick_system`] and
/// the facade's forced-capture path (`MindCampaign::capture_sector`).
///
/// Java's listeners run inside `Logic.sectorCapture` (`Logic.java:394-425`),
/// which fires the event **before** clearing `rules.attackMode`: the service
/// hook therefore receives the pre-capture `was_attack_mode` and
/// `buildings_destroyed` values the caller snapshots before `sector_capture`.
/// The `Logic.java:117-121` listener queues the captured sector's wave team to
/// turn derelict on the next tick.
pub fn on_captured_sector(
    runtime: &mut CampaignRuntime,
    planet: PlanetId,
    was_attack_mode: bool,
    buildings_destroyed: i32,
) {
    runtime.service.on_sector_capture(
        &runtime.session,
        &runtime.campaign,
        planet,
        was_attack_mode,
        buildings_destroyed,
    );
    runtime.pending_derelict_team = Some(runtime.session.rules.wave_team);
}

/// `state.isCampaign() && state.getSector().isCaptured()` for the currently
/// played sector (`Sector.isCaptured`, `Sector.java:181-184`: the played sector
/// has waves and attack mode off).
pub fn current_sector_captured(world: &World) -> bool {
    world
        .get_resource::<CampaignRuntime>()
        .is_some_and(|runtime| {
            runtime.session.is_campaign()
                && !runtime.session.rules.waves
                && !runtime.session.rules.attack_mode
        })
}

/// Forced `Logic.sectorCapture` on the played sector: the runtime half of the
/// facade's `capture_sector` (`Control`'s forced capture path).
///
/// Snapshots the `SectorCaptureEvent` listener inputs **before** the capture
/// (Java fires the event inside `Logic.sectorCapture` while `rules.attackMode`
/// is still set, `Logic.java:394-425`), runs [`sector_capture`], then applies
/// the capture arm's bookkeeping ([`on_captured_sector`]: the service hook and
/// the derelict-wave-team queue) and queues the ordered events.
///
/// Returns whether a capture happened. 02-FINDINGS #2 names this the shared
/// path; X01 keeps the facade's own body until that fix lands, so the facade
/// does not adopt it yet.
pub fn capture_played_sector(runtime: &mut CampaignRuntime) -> bool {
    let was_attack_mode = runtime.session.rules.attack_mode;
    let buildings_destroyed = runtime.session.stats.buildings_destroyed;
    let events = sector_capture(&mut runtime.session, &mut runtime.campaign);
    let captured = !events.is_empty();
    for event in events {
        if let PlayEvent::SectorCapture { planet, .. } = &event {
            on_captured_sector(runtime, *planet, was_attack_mode, buildings_destroyed);
        }
        runtime.events.push(event);
    }
    captured
}
