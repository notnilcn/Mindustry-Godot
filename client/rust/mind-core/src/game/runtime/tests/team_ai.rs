// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Team AI: RTS orders, the base builder and `estimateStats` tests.
//! Upstream: none (port-only tests).

use crate::content::UnitTypeId;
use crate::ecs::{BuildingComp, TeamId};
use crate::entities::comp::Pos;
use crate::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
use crate::game::State;
use crate::game::runtime::team_ai::TeamAiBuildingRow;
use crate::game::runtime::team_ai::builder::{live_type_count, run_team_base_builder};
use crate::game::runtime::team_ai::stats::turret_enemy_stats;
use crate::game::runtime::test_support::*;
use crate::game::runtime::*;
use crate::world::TilePos;
use crate::world::block::BlockTable;
use crate::world::modules::ItemModule;
use bevy_ecs::entity::Entity;

#[test]
fn team_ai_orders_idle_wave_units_at_an_enemy_core() {
    use crate::ai::ControllerSlot;
    use crate::sim::unit_runtime::{UnitRuntime, unit_update_system};
    use crate::world::WorldGrid;

    let registry = {
        let mut registry = content();
        registry.init().expect("init");
        registry
    };
    let table = crate::world::block::BlockTable::build_default(&registry).expect("table");
    let mut world = World::new();

    // Live wave-team units near the map corner.
    let grid = WorldGrid::new(32, 32);
    let runtime_content = {
        let mut content = content();
        content.init().expect("init");
        content
    };
    let mut unit_runtime = UnitRuntime::new(runtime_content, &grid, &world, 2, false);
    let a = unit_runtime
        .spawn(&mut world, "dagger", 2, 8.0, 8.0, 0.0)
        .expect("unit a");
    let b = unit_runtime
        .spawn(&mut world, "dagger", 2, 24.0, 8.0, 0.0)
        .expect("unit b");
    world.insert_non_send(unit_runtime);

    // The player core is the enemy target (BuildingComp mirrors the live
    // `SimHooks::new_building` insert).
    let core_inst = table.get_named("core-shard").expect("core-shard").clone();
    let core = core_inst.spawn(
        &mut world,
        1,
        TilePos::new(20, 20),
        1,
        0,
        registry.items().len(),
        registry.liquids().len(),
    );
    world.entity_mut(core).insert(BuildingComp {
        pos: TilePos::new(20, 20),
        block: registry.block_id("core-shard").expect("core-shard id"),
        team: TeamId(1),
        rot: 0,
    });
    world.insert_resource(table);

    let campaign = campaign(&registry);
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    {
        let rule = session.rules.team_rule_mut(2);
        rule.rts_ai = true;
        rule.rts_min_squad = 1;
    }
    world.insert_resource(CampaignRuntime::new(session, campaign));

    team_ai_system(&mut world);

    let slot_a = world.get::<ControllerSlot>(a).expect("slot a").target;
    let slot_b = world.get::<ControllerSlot>(b).expect("slot b").target;
    assert_eq!(
        slot_a,
        Some(TilePos::new(20, 20)),
        "the squad attacks the enemy core ({slot_a:?})"
    );
    assert_eq!(slot_b, slot_a, "both clustered units share the order");

    // The order drives the live movement system. T4: the first tick only
    // writes `vel` (`VelComp.update` integrates at the start of the next
    // tick), so run two ticks before reading the position.
    let before = world.get::<Pos>(a).expect("pos a").x;
    unit_update_system(&mut world);
    unit_update_system(&mut world);
    let after = world.get::<Pos>(a).expect("pos a").x;
    assert!(
        after > before,
        "ordered unit moves toward the core ({before} -> {after})"
    );
}

#[test]
fn team_ai_base_builder_queues_plans_for_the_host() {
    use crate::ecs::BuildingComp;
    use crate::sim::unit_runtime::UnitRuntime;
    use crate::world::WorldGrid;
    use crate::world::limits::{BlockCounter, BuildRules};

    let registry = {
        let mut registry = content();
        registry.init().expect("init");
        registry
    };
    let table = BlockTable::build_default(&registry).expect("table");
    let mut world = World::new();
    let grid = WorldGrid::new(32, 32);
    let runtime_content = {
        let mut content = content();
        content.init().expect("init");
        content
    };
    world.insert_non_send(UnitRuntime::new(runtime_content, &grid, &world, 2, false));
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
    world.insert_resource(BaseCatalog::default());
    let queues = install_campaign_ai_queues(&mut world);

    let campaign = campaign(&registry);
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    session.rules.team_rule_mut(2).build_ai = true;
    session.teams.register_core(core, TeamId(2), &session.rules);
    // A pending plan from the base builder (its `data.plans` queue).
    session
        .teams
        .get(TeamId(2))
        .plans
        .push_back(crate::game::teams::BlockPlan {
            x: 5,
            y: 5,
            rotation: 0,
            block: registry.block_id("copper-wall").expect("wall"),
            config: None,
            removed: false,
        });
    world.insert_resource(CampaignRuntime::new(session, campaign));

    team_ai_system(&mut world);

    let runtime = world.remove_resource::<CampaignRuntime>().expect("runtime");
    let data = runtime
        .session
        .teams
        .get_or_null(TeamId(2))
        .expect("team data");
    assert!(data.build_ai.is_some(), "the base builder is created");
    assert!(data.plans.is_empty(), "queued plans drained");
    let queued = queues.lock().expect("queue").clone();
    assert_eq!(queued.len(), 1, "one placement request queued");
    assert_eq!(queued[0].team, 2);
    assert_eq!((queued[0].x, queued[0].y), (5, 5));
    world.insert_resource(runtime);
}

/// T10 scaffolding: a world with the live unit/block runtimes, a team-2
/// `core-shard` at (16, 16) and the resources `team_ai_system` reads.
fn t10_team_world() -> (World, Entity, Campaign) {
    use crate::ecs::BuildingComp;
    use crate::sim::unit_runtime::UnitRuntime;
    use crate::world::WorldGrid;
    use crate::world::limits::{BlockCounter, BuildRules};

    let registry = crate::sim::runtime::load_vanilla_content().expect("content");
    let table = BlockTable::build_default(&registry).expect("table");
    let mut world = World::new();
    let grid = WorldGrid::new(32, 32);
    let runtime_content = crate::sim::runtime::load_vanilla_content().expect("content");
    world.insert_non_send(UnitRuntime::new(runtime_content, &grid, &world, 2, false));
    let core_inst = table.get_named("core-shard").expect("core-shard").clone();
    let core = core_inst.spawn(
        &mut world,
        1,
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
    world.insert_resource(BaseCatalog::default());
    let campaign = campaign(&registry);
    (world, core, campaign)
}

fn t10_session(
    world: &mut World,
    core: Entity,
    campaign: Campaign,
    configure: impl FnOnce(&mut crate::game::rules::TeamRule),
) {
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    configure(session.rules.team_rule_mut(2));
    session.teams.register_core(core, TeamId(2), &session.rules);
    world.insert_resource(CampaignRuntime::new(session, campaign));
}

fn alpha_id(world: &World) -> UnitTypeId {
    world
        .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .expect("unit runtime")
        .content
        .unit_by_name("alpha")
        .expect("alpha")
        .id
}

/// T10 (`BaseBuilderAI.java:53-58`): a `buildAi` team's first core is
/// refilled to `getMaximumAccepted` every update — no planet/hidden filter,
/// unlike the `fillItems` rule.
#[test]
fn base_builder_fills_enemy_core() {
    use crate::ecs::BuildingComp;

    let (mut world, core, campaign) = t10_team_world();
    t10_session(&mut world, core, campaign, |rule| rule.build_ai = true);
    let mut runtime = world.remove_resource::<CampaignRuntime>().expect("runtime");
    let rule = runtime.session.rules.team_rule(2).clone();
    let rules = runtime.session.rules.clone();
    run_team_base_builder(&mut world, &mut runtime, TeamId(2), &rule, &rules);
    let copper = world
        .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .expect("unit runtime")
        .content
        .item_id("copper")
        .expect("copper");
    let expected = {
        let block = world.get::<BuildingComp>(core).expect("building").block;
        let table = world.get_resource::<BlockTable>().expect("table");
        let instance = table.instance(block).expect("instance");
        instance.behavior.get_maximum_accepted(&world, core, copper)
    };
    assert!(expected > 0, "the core accepts copper");
    let module = world.get::<ItemModule>(core).expect("core items");
    assert_eq!(module.get(copper), expected, "filled to maximum accepted");
    world.insert_resource(runtime);
}

/// T10 (`BaseBuilderAI.java:61-70`): the 360-tick spawn timer creates core
/// units up to `cores.size * coreUnitMultiplier` (= 2 per core).
#[test]
fn ai_core_spawn_caps_at_two_per_core() {
    let (mut world, core, campaign) = t10_team_world();
    t10_session(&mut world, core, campaign, |rule| rule.build_ai = true);
    let mut runtime = world.remove_resource::<CampaignRuntime>().expect("runtime");
    let rule = runtime.session.rules.team_rule(2).clone();
    let rules = runtime.session.rules.clone();
    let alpha = alpha_id(&world);
    // Three 360-tick spawn windows (`timerSpawn`, `60 * 6`).
    for _ in 0..3 * 360 + 1 {
        run_team_base_builder(&mut world, &mut runtime, TeamId(2), &rule, &rules);
    }
    assert_eq!(
        live_type_count(&world, 2, alpha),
        2,
        "cores.size * coreUnitMultiplier = 2"
    );
    for _ in 0..720 {
        run_team_base_builder(&mut world, &mut runtime, TeamId(2), &rule, &rules);
    }
    assert_eq!(live_type_count(&world, 2, alpha), 2, "the cap holds");
    world.insert_resource(runtime);
}

/// T10 (`RtsAI.java:86-99`): the RtsAI core-unit spawn runs off the
/// 120-tick gate even when the team has no idle squad.
#[test]
fn rts_ai_spawns_core_unit_without_idle_squads() {
    let (mut world, core, campaign) = t10_team_world();
    t10_session(&mut world, core, campaign, |rule| rule.rts_ai = true);
    let alpha = alpha_id(&world);
    for _ in 0..500 {
        team_ai_system(&mut world);
    }
    assert_eq!(
        live_type_count(&world, 2, alpha),
        1,
        "RtsAI.checkBuilding spawned the core unit"
    );
}

/// T10 (`Logic.java:574-584`): `prebuildAi` spawns one core-unit defender
/// per core and flags it with the core's packed tile position.
#[test]
fn prebuild_ai_spawns_one_defender_per_core() {
    let (mut world, core, campaign) = t10_team_world();
    t10_session(&mut world, core, campaign, |rule| rule.prebuild_ai = true);
    let alpha = alpha_id(&world);
    team_ai_system(&mut world);
    assert_eq!(live_type_count(&world, 2, alpha), 1, "one defender");
    let flag = world
        .iter_entities()
        .find(|entity| {
            entity
                .get::<UnitTypeComp>()
                .is_some_and(|comp| comp.type_id == alpha)
        })
        .and_then(|entity| entity.get::<UnitCore>().map(|unit| unit.flag))
        .expect("defender flag");
    assert_eq!(
        flag,
        TilePos::new(16, 16).pack() as f64,
        "unit.flag = core.pos()"
    );
    team_ai_system(&mut world);
    assert_eq!(
        live_type_count(&world, 2, alpha),
        1,
        "the flag prevents a respawn"
    );
}

/// T10 (`RtsAI.java:329-335`, `Turret.java:335-338`): `estimateStats` gains
/// a turret row per live enemy turret with its ammo-based DPS.
#[test]
fn estimate_stats_counts_loaded_turret() {
    use crate::world::blocks::defense::turrets::{TurretState, handle_item, peek_ammo};

    let (mut world, _core, _campaign) = t10_team_world();
    let duo_inst = world
        .get_resource::<BlockTable>()
        .expect("table")
        .get_named("duo")
        .expect("duo")
        .clone();
    let duo = duo_inst.spawn(&mut world, 2, TilePos::new(20, 20), 1, 0, 0, 0);
    let copper = world
        .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .expect("unit runtime")
        .content
        .item_id("copper")
        .expect("copper");
    for _ in 0..10 {
        handle_item(&mut world, duo, copper);
    }
    let row = TeamAiBuildingRow {
        entity: duo,
        seq: 5,
        team: 1,
        pos: TilePos::new(20, 20),
        x: 160.0,
        y: 160.0,
        in_core: false,
        flag_rank: None,
        damaged: false,
    };
    // Without ammo the same turret contributes 0 (`hasAmmo()` gate).
    let empty = duo_inst.spawn(&mut world, 3, TilePos::new(24, 20), 1, 0, 0, 0);
    let empty_row = TeamAiBuildingRow {
        entity: empty,
        ..row
    };
    let content = &world
        .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .expect("unit runtime")
        .content;
    let stats = turret_enemy_stats(&world, content, &[row], 2);
    assert_eq!(stats.len(), 1, "one enemy turret row");
    assert!(stats[0].is_turret, "row is tagged as a turret");
    let expected = {
        let state = world.get::<TurretState>(duo).expect("turret state");
        let bullet = peek_ammo(&world, duo).expect("ammo peek");
        let def = content.bullet(bullet).expect("bullet");
        state.config.shoot.shots as f32 / state.config.reload * 60.0 * def.estimate_dps
    };
    assert!(expected > 0.0, "a loaded duo has DPS");
    assert!(
        (stats[0].dps - expected).abs() < 0.001,
        "dps {} == {expected}",
        stats[0].dps
    );
    let stats = turret_enemy_stats(&world, content, &[empty_row], 2);
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].dps, 0.0, "no ammo -> 0 DPS");
}
