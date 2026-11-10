// SPDX-License-Identifier: GPL-3.0-only

//! #65: the `team_ai_system` host applies `BaseBuilderAI`'s
//! `fill_core_items`/`spawn_core_unit` actions.
//!
//! Upstream: none (port-only tests).

use crate::sim::*;

fn host_world() -> (bevy_ecs::world::World, Entity) {
    use crate::ecs::BuildingComp;
    use crate::sim::unit_runtime::UnitRuntime;
    use crate::world::WorldGrid;
    use crate::world::limits::{BlockCounter, BuildRules};

    let registry = crate::world::harness::BuildHarness::load_content();
    let table = crate::world::block::BlockTable::build_default(&registry).expect("table");
    let mut world = bevy_ecs::world::World::new();
    let grid = WorldGrid::new(32, 32);
    let runtime_content = crate::world::harness::BuildHarness::load_content();
    let runtime = UnitRuntime::new(runtime_content, &grid, &world, 2, false);
    world.insert_non_send(runtime);
    let core_inst = table.get_named("core-shard").expect("core-shard").clone();
    let core = core_inst.spawn(
        &mut world,
        3,
        TilePos::new(16, 16),
        2,
        0,
        registry.items().len(),
        registry.liquids().len(),
    );
    world.entity_mut(core).insert(BuildingComp {
        pos: TilePos::new(16, 16),
        block: registry.block_id("core-shard").expect("core id"),
        team: TeamId(2),
        rot: 0,
    });
    world.insert_resource(table);
    world.insert_resource(BuildRules::default());
    world.insert_resource(BlockCounter::new());
    world.insert_resource(crate::game::runtime::BaseCatalog::default());
    (world, core)
}

fn host_campaign(world: &mut bevy_ecs::world::World, core: Entity) {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let content = crate::world::harness::BuildHarness::load_content();
    let campaign = Campaign::from_registry(&content, &EmptyNeighborhood);
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    let rule = session.rules.team_rule_mut(2);
    rule.build_ai = true;
    rule.fill_items = true;
    rule.ai_core_spawn = true;
    session.teams.register_core(core, TeamId(2), &session.rules);
    world.insert_resource(CampaignRuntime::new(session, campaign));
}

/// One host `team_ai_system` call fills the team's first core with every
/// item up to `getMaximumAccepted` (`BaseBuilderAI.java:53-58`).
#[test]
fn base_builder_host_fills_the_first_core() {
    use crate::world::modules::ItemModule;

    let (mut world, core) = host_world();
    host_campaign(&mut world, core);
    assert_eq!(
        world.get::<ItemModule>(core).map(|items| items.total),
        Some(0)
    );
    crate::game::runtime::team_ai_system(&mut world);
    let copper = crate::world::harness::BuildHarness::load_content()
        .item_id("copper")
        .expect("copper");
    let total = world
        .get::<ItemModule>(core)
        .map(|items| items.get(copper))
        .unwrap_or(0);
    assert!(total > 0, "the host filled the core (copper {total})");
}

/// After the `BaseBuilderAI` 360-tick spawn interval the host creates the
/// core unit at a random core (`BaseBuilderAI.java:61-72`).
#[test]
fn base_builder_host_spawns_a_core_unit() {
    use crate::entities::comp::TeamComp;
    use crate::entities::comp::unit::comp::UnitTypeComp;
    use crate::sim::unit_runtime::UnitRuntime;

    let (mut world, core) = host_world();
    host_campaign(&mut world, core);
    for _ in 0..360 {
        crate::game::runtime::team_ai_system(&mut world);
    }
    let alpha = crate::world::harness::BuildHarness::load_content()
        .unit_id("alpha")
        .expect("alpha");
    let runtime = world.get_non_send::<UnitRuntime>().expect("unit runtime");
    let spawned = runtime
        .units
        .iter()
        .filter(|entity| {
            world
                .get::<UnitTypeComp>(**entity)
                .is_some_and(|comp| comp.type_id == alpha)
                && world
                    .get::<TeamComp>(**entity)
                    .is_some_and(|comp| comp.team == 2)
        })
        .count();
    assert_eq!(spawned, 1, "the host spawned the core unit");
}
