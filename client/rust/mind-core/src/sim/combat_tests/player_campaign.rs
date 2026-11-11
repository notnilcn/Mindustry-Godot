// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Player team/spawn and campaign-spawn regressions.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;
use crate::sim::test_support::{live_sim, place_at, tick_n};

/// T3: `PlayerComp.team` is set before the spawn attempt, so a player without
/// a supported core retries `bestCore()` every 60 ticks
/// (`PlayerComp.java:214-243`) instead of staying teamless forever.
#[test]
fn player_team_set_without_core_retries_spawn() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    sim.set_player_team(1);
    tick_n(&mut sim, 120);
    assert!(sim.player_unit().is_none(), "no player unit without a core");
    // Landing a core lets the next 60-tick window spawn the unit.
    let _core = place_at(&mut sim, 20, 20, "core-shard", 1);
    tick_n(&mut sim, 60);
    let player = sim.player_unit().expect("player spawned at the new core");
    let pos = sim.ecs.0.get::<Pos>(player).expect("pos");
    assert_eq!((pos.x, pos.y), (160.0, 160.0), "spawned on the core");
}

/// T3/PAR-0175: the `unitClear` dock branch releases the possessed unit before
/// the docked unit takes the player controller (`InputHandler.java:823-873`):
/// exactly one entity keeps `comp::Player`.
#[test]
fn clear_unit_dock_releases_possessed_unit() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::entities::comp::Player;

    let mut sim = live_sim(48);
    if let Some(mut rules) = sim
        .ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
    {
        // Team 1 must be the player team (`select_team_controller` gives
        // `Command` only to the player's team; the rest get the AI branch).
        rules.default_team = 1;
    }
    let _core = place_at(&mut sim, 20, 20, "core-shard", 1);
    // The dockable types are the `core_unit_dock` units (Java: evoke, incite,
    // emanate, UnitTypes.java:4355/4421/4500) - the shard's alpha is not one.
    let dock_name = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime
            .content
            .units()
            .iter()
            .find(|unit| unit.core_unit_dock)
            .map(|unit| unit.name.clone())
            .expect("a coreUnitDock unit exists")
    };
    eprintln!("T3 dock unit: {dock_name}");
    let player = sim
        .spawn_player_unit("alpha", 1, 160.0, 160.0, 90.0)
        .expect("player alpha");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 200.0, 160.0, 0.0)
        .expect("dagger");
    let dagger_seq = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    // Force the dockable type so the dock branch is exercised on this map.
    let dock_id = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_by_name(&dock_name)
        .map(|def| def.id)
        .expect("dock unit id");
    if let Some(mut docked) = sim
        .ecs
        .0
        .get_mut::<crate::entities::comp::unit::comp::DockedType>(dagger)
    {
        docked.0 = dock_id;
    }
    let _ = player;
    assert!(sim.possess_unit(dagger_seq), "dagger possessed");
    assert_eq!(
        sim.ecs
            .0
            .get::<ControllerSlot>(dagger)
            .map(|slot| slot.kind),
        Some(AiKind::Player)
    );
    assert!(sim.clear_unit(), "unitClear ran the dock branch");
    // Exactly one entity carries the Player marker, and the dagger is back on
    // its team AI.
    let players = sim
        .ecs
        .0
        .iter_entities()
        .filter(|entity| entity.get::<Player>().is_some())
        .count();
    assert_eq!(players, 1, "exactly one player entity");
    assert_eq!(
        sim.ecs
            .0
            .get::<ControllerSlot>(dagger)
            .map(|slot| slot.kind),
        Some(AiKind::Command),
        "the possessed dagger went back to its team controller"
    );
}

/// T3: `CoreBlock.playerSpawn` queues `spawnType.unlock()` in the campaign
/// (`CoreBlock.java:121-123`) through H04 T2's `UnlockRef` pipeline.
#[test]
fn campaign_spawn_queues_unlock() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::{CampaignCoreQueues, CampaignRuntime, UnlockRef};
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        ..Rules::default()
    });
    session.sector = sim
        .unit_runtime()
        .expect("rt")
        .content
        .planet_id("serpulo")
        .map(|planet| (planet, 170));
    assert!(session.is_campaign(), "the play session is a campaign run");
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    let queues = crate::game::runtime::install_campaign_core_hooks(&mut sim.ecs.0);
    assert!(
        sim.ecs.0.get_resource::<CampaignCoreQueues>().is_some(),
        "campaign hook queues installed"
    );
    let _core = place_at(&mut sim, 20, 20, "core-shard", 1);
    let alpha = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_id("alpha")
        .expect("alpha");
    sim.spawn_player_at_best_core(1).expect("player spawned");
    let unlocked: Vec<UnlockRef> = queues.lock().expect("lock").unlocks.clone();
    assert_eq!(unlocked, vec![UnlockRef::Unit(alpha)], "alpha queued once");
    // A second spawn must not queue a duplicate.
    sim.player_unit = None;
    sim.spawn_player_at_best_core(1)
        .expect("player spawned again");
    let unlocked: Vec<UnlockRef> = queues.lock().expect("lock").unlocks.clone();
    assert_eq!(unlocked, vec![UnlockRef::Unit(alpha)], "no duplicate");
}
