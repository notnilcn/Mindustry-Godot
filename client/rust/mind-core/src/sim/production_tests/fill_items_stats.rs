// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime core item-fill and game-stat regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;
use crate::sim::test_support::{
    campaign_runtime, campaign_runtime_mut, install_campaign_session, item, place_team1,
    runtime_sim, stored_item, tick_n,
};

/// H09 T8: `TeamRule.fillItems` refills the first core of every active team
/// (`Logic.java:552-561`): with `coreIncinerates` the max accepted is
/// `i32::MAX / 2`, hidden items stay 0 and without incineration the core's
/// storage capacity is the cap.
#[test]
fn fill_items_fills_ai_core() {
    use crate::entities::comp::Building;
    use crate::game::rules::Rules;
    use crate::game::runtime::{
        CampaignRuntime, install_campaign_core_hooks, sync_session_with_sim,
    };
    use crate::world::TilePos;

    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let mut rules = Rules {
        default_team: 1,
        ..Rules::default()
    };
    rules.team_rule_mut(2).fill_items = true;
    rules.core_incinerates = true;
    install_campaign_session(&mut sim, rules);
    install_campaign_core_hooks(&mut sim.ecs.0);
    // `PlaySession` starts in `Menu`; the campaign systems only run in
    // `Playing`.
    if let Some(mut runtime) = sim.ecs.0.get_resource_mut::<CampaignRuntime>() {
        runtime.session.phase = crate::game::State::Playing;
    }
    let core = BlockId::new(sim.world_block_id("core-shard").expect("core-shard"));
    // API staging form (`player: None`): the player core-placement gate
    // (`CoreBlock.canPlaceOn`, H02 T4) rightly refuses a fresh core on free
    // ground through the player form.
    sim.command(crate::determinism::SimCommand::Place {
        x: 5,
        y: 5,
        block: core.raw(),
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("team-2 core placed");
    {
        let content = crate::world::harness::BuildHarness::load_content();
        let mut runtime = sim
            .ecs
            .0
            .remove_resource::<CampaignRuntime>()
            .expect("runtime");
        let _ = sync_session_with_sim(&mut runtime.session, &mut sim.ecs.0, &content, 0, false);
        sim.ecs.0.insert_resource(runtime);
    }
    tick_n(&mut sim, 2);
    let entity = sim.grid.entity_at(TilePos::new(5, 5)).expect("core entity");
    let copper = item(&sim, "copper");
    assert_eq!(
        stored_item(&sim, entity, copper),
        i32::MAX / 2,
        "coreIncinerates -> getMaximumAccepted = i32::MAX / 2"
    );
    // A hidden item is never filled.
    let hidden = {
        let content = sim.block_runtime().expect("rt").content();
        content
            .items()
            .iter()
            .find(|item| item.is_hidden())
            .map(|item| item.id)
    };
    if let Some(hidden) = hidden {
        assert_eq!(
            stored_item(&sim, entity, hidden),
            0,
            "hidden items are skipped"
        );
    }
    assert!(
        sim.ecs.0.get::<Building>(entity).is_some(),
        "the core still exists"
    );

    // Without `coreIncinerates` the cap is the core's storage capacity.
    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let mut rules = Rules {
        default_team: 1,
        ..Rules::default()
    };
    rules.team_rule_mut(2).fill_items = true;
    rules.core_incinerates = false;
    install_campaign_session(&mut sim, rules);
    install_campaign_core_hooks(&mut sim.ecs.0);
    if let Some(mut runtime) = sim.ecs.0.get_resource_mut::<CampaignRuntime>() {
        runtime.session.phase = crate::game::State::Playing;
    }
    let core = BlockId::new(sim.world_block_id("core-shard").expect("core-shard"));
    sim.command(crate::determinism::SimCommand::Place {
        x: 5,
        y: 5,
        block: core.raw(),
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("team-2 core placed");
    {
        let content = crate::world::harness::BuildHarness::load_content();
        let mut runtime = sim
            .ecs
            .0
            .remove_resource::<CampaignRuntime>()
            .expect("runtime");
        let _ = sync_session_with_sim(&mut runtime.session, &mut sim.ecs.0, &content, 0, false);
        sim.ecs.0.insert_resource(runtime);
    }
    tick_n(&mut sim, 2);
    let entity = sim.grid.entity_at(TilePos::new(5, 5)).expect("core entity");
    let capacity = sim
        .ecs
        .0
        .get::<crate::world::blocks::storage::core_block::CoreBuild>(entity)
        .map(|core| core.storage_capacity)
        .expect("core build");
    assert_eq!(
        stored_item(&sim, entity, copper),
        capacity,
        "without incineration the core fills to its storage capacity"
    );
}

/// H06 T20 blocker regression: `team_ai_system` must skip teams whose rule
/// leaves `fillItems` false (`Logic.java:553-554`). The missing gate filled
/// the player team's core with `i32::MAX / 2` of every item every tick and
/// wrote an unloadable sector save.
#[test]
fn fill_items_gate_skips_other_teams() {
    use crate::entities::comp::Building;
    use crate::game::rules::Rules;
    use crate::game::runtime::{
        CampaignRuntime, install_campaign_core_hooks, sync_session_with_sim,
    };
    use crate::world::TilePos;

    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let rules = Rules {
        default_team: 1,
        core_incinerates: true,
        ..Rules::default()
    };
    install_campaign_session(&mut sim, rules);
    install_campaign_core_hooks(&mut sim.ecs.0);
    if let Some(mut runtime) = sim.ecs.0.get_resource_mut::<CampaignRuntime>() {
        runtime.session.phase = crate::game::State::Playing;
    }
    let core = BlockId::new(sim.world_block_id("core-shard").expect("core-shard"));
    sim.command(SimCommand::Place {
        x: 5,
        y: 5,
        block: core.raw(),
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("team-2 core placed");
    {
        let content = crate::world::harness::BuildHarness::load_content();
        let mut runtime = sim
            .ecs
            .0
            .remove_resource::<CampaignRuntime>()
            .expect("runtime");
        let _ = sync_session_with_sim(&mut runtime.session, &mut sim.ecs.0, &content, 0, false);
        sim.ecs.0.insert_resource(runtime);
    }
    let copper = item(&sim, "copper");
    tick_n(&mut sim, 2);
    let entity = sim.grid.entity_at(TilePos::new(5, 5)).expect("core entity");
    assert_eq!(
        stored_item(&sim, entity, copper),
        0,
        "team 2 leaves fillItems off: the core is untouched"
    );
    // Flip the rule: the same core is refilled.
    campaign_runtime_mut(&mut sim)
        .session
        .rules
        .team_rule_mut(2)
        .fill_items = true;
    tick_n(&mut sim, 2);
    assert_eq!(
        stored_item(&sim, entity, copper),
        i32::MAX / 2,
        "fillItems on: refilled to getMaximumAccepted"
    );
    assert!(sim.ecs.0.get::<Building>(entity).is_some(), "core exists");
}

/// PAR-0249 regression (found by the H03 session): only teams whose rule sets
/// `fillItems` may be refilled (`Logic.java:554`
/// `if(rules.fillItems && data.cores.size > 0)`). The campaign sets it on the
/// wave team alone (`apply_world_load_campaign_rules`), so the PLAYER core must
/// stay empty — filling it wrote `i32::MAX / 2` per item (`coreIncinerates`)
/// into every exit save.
#[test]
fn fill_items_does_not_fill_player_core() {
    use crate::game::rules::Rules;
    use crate::game::runtime::{
        CampaignRuntime, install_campaign_core_hooks, sync_session_with_sim,
    };
    use crate::world::TilePos;

    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let mut rules = Rules {
        default_team: 1,
        ..Rules::default()
    };
    // Campaign shape: only the wave team's rule gets `fillItems`.
    rules.team_rule_mut(1).fill_items = false;
    rules.team_rule_mut(2).fill_items = true;
    rules.core_incinerates = true;
    install_campaign_session(&mut sim, rules);
    install_campaign_core_hooks(&mut sim.ecs.0);
    if let Some(mut runtime) = sim.ecs.0.get_resource_mut::<CampaignRuntime>() {
        runtime.session.phase = crate::game::State::Playing;
    }
    let core = BlockId::new(sim.world_block_id("core-shard").expect("core-shard"));
    for (x, y, team) in [(5, 5, 1u8), (15, 15, 2u8)] {
        sim.command(crate::determinism::SimCommand::Place {
            x,
            y,
            block: core.raw(),
            rotation: 0,
            team,
            player: None,
        })
        .expect("core placed");
    }
    {
        let content = crate::world::harness::BuildHarness::load_content();
        let mut runtime = sim
            .ecs
            .0
            .remove_resource::<CampaignRuntime>()
            .expect("runtime");
        let _ = sync_session_with_sim(&mut runtime.session, &mut sim.ecs.0, &content, 0, false);
        sim.ecs.0.insert_resource(runtime);
    }
    tick_n(&mut sim, 2);
    let copper = item(&sim, "copper");
    let player_core = sim.grid.entity_at(TilePos::new(5, 5)).expect("player core");
    let enemy_core = sim
        .grid
        .entity_at(TilePos::new(15, 15))
        .expect("enemy core");
    assert_eq!(
        stored_item(&sim, player_core, copper),
        0,
        "the player core (fillItems=false) is never refilled"
    );
    assert_eq!(
        stored_item(&sim, enemy_core, copper),
        i32::MAX / 2,
        "the wave team's core is still filled (fillItems=true)"
    );
}

/// H09 T10: the `Logic` event counters (`Logic.java:186-216`) follow player
/// builds, deconstructs and losses.
#[test]
fn game_stats_count_build_break_destroy() {
    use crate::command::Command;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;

    let mut sim = runtime_sim();
    install_campaign_session(
        &mut sim,
        Rules {
            default_team: 1,
            ..Rules::default()
        },
    );
    if let Some(mut runtime) = sim.ecs.0.get_resource_mut::<CampaignRuntime>() {
        runtime.session.phase = crate::game::State::Playing;
    }
    let wall = BlockId::new(sim.world_block_id("copper-wall").expect("wall"));

    // Two player-team walls; the API place path emits `BlockPlacedEvent`
    // (`BlockBuildEndEvent`).
    assert!(sim.place_for_team(5, 5, wall, 0, 1));
    assert!(sim.place_for_team(7, 5, wall, 0, 1));
    // Deconstruct one through the break command (`BlockBuildEndEvent{breaking}`).
    sim.apply(Command::Break { x: 5, y: 5 }).expect("break");
    // Destroy one player wall and one enemy wall (`BlockDestroyEvent`).
    let player_wall = sim.grid.entity_at(TilePos::new(7, 5)).expect("player wall");
    if let Some(mut health) = sim
        .ecs
        .0
        .get_mut::<crate::entities::comp::Health>(player_wall)
    {
        health.health = 0.0;
    }
    assert!(sim.place_for_team(9, 5, wall, 0, 2));
    let enemy_wall = sim.grid.entity_at(TilePos::new(9, 5)).expect("enemy wall");
    if let Some(mut health) = sim
        .ecs
        .0
        .get_mut::<crate::entities::comp::Health>(enemy_wall)
    {
        health.health = 0.0;
    }
    // One tick resolves the deaths and flushes the events.
    sim.tick().expect("tick");
    sim.tick().expect("tick");

    let stats = &campaign_runtime(&sim).session.stats;
    assert_eq!(stats.buildings_built, 2, "two walls built");
    assert_eq!(stats.buildings_deconstructed, 1, "one wall deconstructed");
    assert_eq!(stats.buildings_destroyed, 1, "one player wall destroyed");
    assert_eq!(
        stats.get_destroyed("copper-wall"),
        1,
        "one enemy wall destroyed by name"
    );
    // `placedBlockCount` only counts the default team's placements.
    assert_eq!(stats.placed_block_count.get("copper-wall"), Some(&2));
}

/// H09 T10: a factory-produced default-team unit bumps `unitsCreated`
/// (`Logic.java:228-235`).
#[test]
fn game_stats_count_units_created() {
    use crate::game::rules::Rules;

    let mut sim = runtime_sim();
    install_campaign_session(
        &mut sim,
        Rules {
            default_team: 1,
            // No core in this fixture; Java's unit-cap check would kill the
            // factory output at spawn.
            disable_unit_cap: true,
            ..Rules::default()
        },
    );
    if let Some(mut runtime) = sim
        .ecs
        .0
        .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
    {
        runtime.session.phase = crate::game::State::Playing;
    }
    let factory = place_team1(&mut sim, "ground-factory", 5, 5);
    let (silicon, lead) = (item(&sim, "silicon"), item(&sim, "lead"));
    {
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(factory) {
            items.add(silicon, 10, 60);
            items.add(lead, 10, 60);
        }
    }
    let _source = place_team1(&mut sim, "power-source", 3, 5);
    let dagger = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    for _ in 0..1500 {
        sim.tick().expect("tick");
        if spawned_unit(&sim, dagger) {
            break;
        }
    }
    assert!(spawned_unit(&sim, dagger), "factory produced a dagger");
    let stats = &campaign_runtime(&sim).session.stats;
    assert_eq!(stats.units_created, 1, "one unit created");
}
