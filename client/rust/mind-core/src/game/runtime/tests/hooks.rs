// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Campaign hook queue folding and unlock-event tests.
//! Upstream: none (port-only tests).

use crate::event::{EventContent, EventRow, GameEvent};
use crate::game::State;
use crate::game::runtime::test_support::*;
use crate::game::runtime::*;

#[test]
fn core_and_production_deltas_fold_into_the_sector_info() {
    let registry = content();
    let campaign = campaign(&registry);
    let planet = campaign.planet_id_by_name("serpulo").unwrap();
    let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.sector = Some((planet, sector_id));
    session.phase = State::Playing;
    let mut world = World::new();
    install_campaign_core_hooks(&mut world);
    world.insert_resource(CampaignRuntime::new(session, campaign));

    // A core accept queues through the installed hooks; a miner tick queues
    // a production delta directly (Java `CoreBlock.handleItem` /
    // `MinerComp.update` -> `SectorInfo.handleCoreItem`/`handleProduction`).
    let hooks = world
        .get_resource::<crate::world::blocks::storage::CoreHooks>()
        .expect("hooks installed")
        .0
        .clone();
    let copper = registry.item_id("copper").unwrap();
    hooks.handle_core_item(copper, 3);
    if let Some(queues) = world.get_resource::<CampaignCoreQueues>()
        && let Ok(mut state) = queues.0.lock()
    {
        state.production_deltas.push((copper, 5));
    }

    campaign_tick_system(&mut world);
    let runtime = world
        .remove_resource::<CampaignRuntime>()
        .expect("runtime installed");
    let sector = runtime.campaign.sector(planet, sector_id).expect("sector");
    assert_eq!(sector.info.windows.core_deltas[copper.index()], 3);
    assert_eq!(sector.info.windows.production_deltas[copper.index()], 5);
}

/// GAPS #37: a produced/dumped unlock queues its `UnlockEvent` row for the
/// host `Events` bus (`UnlockableContent.unlock` -> `Control.java:160-174`),
/// deduped against the still-pending unlocks.
#[test]
fn produced_unlocks_queue_an_unlock_event_row() {
    let registry = content();
    let campaign = campaign(&registry);
    let planet = campaign.planet_id_by_name("serpulo").unwrap();
    let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.sector = Some((planet, sector_id));
    session.phase = State::Playing;
    let mut world = World::new();
    install_campaign_core_hooks(&mut world);
    world.insert_resource(CampaignRuntime::new(session, campaign));

    let copper = registry.item_id("copper").unwrap();
    if let Some(queues) = world.get_resource::<CampaignCoreQueues>()
        && let Ok(mut state) = queues.0.lock()
    {
        state.unlocks.push(UnlockRef::Item(copper));
    }
    campaign_tick_system(&mut world);
    {
        let mut runtime = world
            .remove_resource::<CampaignRuntime>()
            .expect("runtime installed");
        assert!(runtime.pending_unlocks.contains(&UnlockRef::Item(copper)));
        let rows = runtime.session.event_rows();
        assert_eq!(rows.len(), 1, "one unlock row: {rows:?}");
        assert_eq!(rows[0].class_name(), "UnlockEvent");
        assert!(matches!(
            rows[0],
            EventRow::Game(GameEvent::UnlockEvent(event))
                if event.content == EventContent::Item(copper.raw())
        ));
        // The host drained the row on its pump.
        let _ = runtime.session.take_event_rows();
        world.insert_resource(runtime);
    }

    // The content is still pending (host has not persisted it): a second
    // queue must not re-fire the row.
    if let Some(queues) = world.get_resource::<CampaignCoreQueues>()
        && let Ok(mut state) = queues.0.lock()
    {
        state.unlocks.push(UnlockRef::Item(copper));
    }
    campaign_tick_system(&mut world);
    let runtime = world
        .remove_resource::<CampaignRuntime>()
        .expect("runtime installed");
    assert!(
        runtime.session.event_rows().is_empty(),
        "deduped unlock fires once: {:?}",
        runtime.session.event_rows()
    );
}
