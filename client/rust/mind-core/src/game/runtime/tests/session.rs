// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Campaign session sync and launch-loadout tests.
//! Upstream: none (port-only tests).

use crate::ecs::TeamId;
use crate::game::runtime::test_support::*;
use crate::game::runtime::*;
use crate::world::TilePos;
use crate::world::block::BlockTable;
use crate::world::modules::ItemModule;

#[test]
fn sync_session_preserves_carried_team_plans() {
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
    let grid = WorldGrid::new(16, 16);
    let runtime_content = {
        let mut content = content();
        content.init().expect("init");
        content
    };
    world.insert_non_send(UnitRuntime::new(runtime_content, &grid, &world, 2, false));
    let core_inst = table.get_named("core-shard").expect("core-shard").clone();
    let core = core_inst.spawn(
        &mut world,
        7,
        TilePos::new(3, 3),
        1,
        0,
        registry.items().len(),
        registry.liquids().len(),
    );
    world.entity_mut(core).insert(BuildingComp {
        pos: TilePos::new(3, 3),
        block: registry.block_id("core-shard").expect("core id"),
        team: TeamId(1),
        rot: 0,
    });
    world.insert_resource(table);
    world.insert_resource(BuildRules::default());
    world.insert_resource(BlockCounter::new());

    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session
        .teams
        .get(TeamId(2))
        .plans
        .push_back(crate::game::teams::BlockPlan {
            x: 9,
            y: 9,
            rotation: 1,
            block: registry.block_id("copper-wall").expect("wall"),
            config: None,
            removed: false,
        });
    let sync = sync_session_with_sim(&mut session, &mut world, &registry, 0, false);
    assert_eq!(sync.player_cores, 1);
    let plans = &session.teams.get_or_null(TeamId(2)).expect("team 2").plans;
    assert_eq!(plans.len(), 1, "carried plans survive the teams rebuild");
    assert_eq!((plans[0].x, plans[0].y), (9, 9));
}

#[test]
fn sync_registers_cores_and_spawns() {
    use crate::ecs::BuildingComp as Comp;
    use crate::world::TilePos;

    let registry = content();
    let mut session = PlaySession::new(Rules {
        waves: true,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    let mut world = World::new();
    let core = registry
        .block_id("core-shard")
        .expect("core-shard in base content");
    world.spawn(Comp {
        pos: TilePos::new(8, 8),
        block: core,
        team: TeamId(1),
        rot: 0,
    });
    world.spawn(Comp {
        pos: TilePos::new(40, 40),
        block: core,
        team: TeamId(2),
        rot: 0,
    });
    let sync = sync_session_with_sim(&mut session, &mut world, &registry, 1, true);
    assert_eq!(sync.player_cores, 1);
    assert_eq!(sync.wave_cores, 1);
    assert_eq!(sync.spawn_count, 1);
    assert_eq!(session.player_core_count(), 1);
    assert_eq!(session.wave_core_count(), 1);
}

#[test]
fn sync_applies_the_launch_loadout() {
    use crate::ecs::BuildingComp as Comp;
    use crate::io::json::content_serde::JsonItemStack;
    use crate::world::TilePos;

    let registry = content();
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.add_starting_items = true;
    session.rules.loadout = vec![JsonItemStack {
        item: Some("copper".to_owned()),
        amount: 500,
    }];
    let mut world = World::new();
    let core = registry.block_id("core-shard").expect("core");
    let entity = world
        .spawn(Comp {
            pos: TilePos::new(8, 8),
            block: core,
            team: TeamId(1),
            rot: 0,
        })
        .id();
    let lead = registry.item_id("lead").expect("lead");
    {
        let mut module = ItemModule::with_items(registry.items().len());
        module.add(lead, 9, 100);
        world.entity_mut(entity).insert(module);
    }
    let _ = sync_session_with_sim(&mut session, &mut world, &registry, 0, true);
    let copper = registry.item_id("copper").expect("copper");
    let inventory = session
        .teams
        .inventory_ref(TeamId(1))
        .expect("team inventory");
    assert_eq!(inventory.get(copper), 500);
    // Upstream also clears/loads the core building's own storage.
    let module = world.get::<ItemModule>(entity).expect("core module");
    assert_eq!(module.get(copper), 500);
    assert_eq!(module.get(lead), 0, "fresh launch clears core storage");
}
