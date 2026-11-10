// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Builder queue/pass tests: player plan progression, core funding, construct
//! progress, team ghost plans, repairs and bridge relinking.
//!
//! Upstream: none (port-only tests).

use super::checks::team_core;
use super::update::chain_bridge;
use super::*;
use crate::determinism::SimCommand;
use crate::sim::test_support::{live_sim_seeded, tick_n};

fn sim_with_player() -> (Sim, Entity, u16, BlockId) {
    let mut sim = live_sim_seeded(7, 64);
    let core_id = sim.world_block_id("core-shard").expect("core");
    sim.command(SimCommand::Place {
        x: 10,
        y: 10,
        block: core_id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("core placed");
    let wall = BlockId::new(sim.world_block_id("copper-wall").expect("wall"));
    let unit = sim
        .spawn_player_unit("alpha", 1, 80.0, 80.0, 0.0)
        .expect("player unit");
    // The local player is team 1 (`sharded`); sync the placement guard like
    // a campaign load does (`sync_build_rules`), so PLAYER-flag queries use
    // the real player team.
    sim.ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("build rules")
        .default_team = 1;
    (sim, unit, core_id, wall)
}

fn core_copper(sim: &Sim) -> i32 {
    let runtime = sim.block_runtime().expect("runtime");
    let copper = runtime.content().item_id("copper").expect("copper");
    let core = team_core(&sim.ecs.0, 1).expect("core");
    sim.ecs
        .0
        .get::<ItemModule>(core)
        .map(|m| m.get(copper))
        .unwrap_or(0)
}

fn set_core_copper(sim: &mut Sim, amount: i32) {
    let copper = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .item_id("copper")
        .expect("copper");
    let core = team_core(&sim.ecs.0, 1).expect("core");
    let mut module = sim.ecs.0.get_mut::<ItemModule>(core).expect("items");
    let have = module.get(copper);
    module.remove(copper, have);
    module.add(copper, amount, 100_000);
}

fn set_core_item(sim: &mut Sim, name: &str, amount: i32) {
    let item = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .item_id(name)
        .expect("item");
    let core = team_core(&sim.ecs.0, 1).expect("core");
    let mut module = sim.ecs.0.get_mut::<ItemModule>(core).expect("items");
    let have = module.get(item);
    module.remove(item, have);
    module.add(item, amount, 100_000);
}

/// Inserts a live `CampaignRuntime` with `default_team = 1` in `Playing`
/// (`sim::combat_tests::destroyed_building_leaves_rebuild_plan` shape).
fn with_campaign(sim: &mut Sim) {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let campaign = {
        let content = &sim.unit_runtime().expect("runtime").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        ..Rules::default()
    });
    session.phase = crate::game::State::Playing;
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
}

/// Team ghost rebuild plans (queue order).
fn team_ghosts(sim: &Sim, team: u8) -> Vec<crate::game::teams::BlockPlan> {
    sim.ecs
        .0
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .and_then(|campaign| campaign.session.teams.get_or_null(crate::ecs::TeamId(team)))
        .map(|data| data.plans.iter().cloned().collect())
        .unwrap_or_default()
}

/// Builds a team-1 wall at `(x, y)` through the player plan queue (so the
/// finish branch runs) and returns its entity.
fn build_wall(sim: &mut Sim, x: i16, y: i16, wall: BlockId) -> Entity {
    assert!(sim.queue_player_place(x, y, wall, 0));
    tick_n(sim, 60);
    assert_eq!(
        sim.grid.block_at(TilePos::new(x, y)),
        Some(wall),
        "wall finished at ({x}, {y})"
    );
    sim.grid.entity_at(TilePos::new(x, y)).expect("wall entity")
}

/// Kills a building (`Health.health = 0`); the next tick's `resolve_deaths`
/// destroys it and pushes its team ghost plan.
fn kill_building(sim: &mut Sim, entity: Entity) {
    sim.ecs
        .0
        .get_mut::<crate::entities::comp::Health>(entity)
        .expect("health")
        .health = 0.0;
    sim.tick().expect("tick");
}

#[test]
fn player_place_builds_over_time_and_spends_core_items() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    set_core_copper(&mut sim, 100);
    sim.command(SimCommand::Place {
        x: 14,
        y: 10,
        block: wall.raw(),
        rotation: 0,
        team: 1,
        player: Some(0),
    })
    .expect("queued");
    assert_eq!(sim.player_build_plans().len(), 1, "plan queued, not placed");
    assert_ne!(sim.grid.block_at(TilePos::new(14, 10)), Some(wall));
    sim.tick().expect("tick");
    let (current, progress, _) = sim
        .construct_progress(14, 10)
        .expect("construct block begun");
    assert_eq!(current, wall);
    assert!(progress > 0.0 && progress < 1.0, "progress {progress}");
    // copper-wall: 6 copper, buildTime = 6 * 0.5 = 3 ticks... alpha 0.5
    // speed -> ~6 ticks.
    tick_n(&mut sim, 60);
    assert_eq!(
        sim.grid.block_at(TilePos::new(14, 10)),
        Some(wall),
        "finished"
    );
    assert!(sim.player_build_plans().is_empty());
    assert_eq!(core_copper(&sim), 94, "requirements drawn from the core");
}

#[test]
fn player_plan_config_applies_on_finish() {
    use crate::world::config::{ConfigValue, read_config};

    let (mut sim, _unit, _core, _wall) = sim_with_player();
    // sorter = 2 lead + 2 copper (build_cost_multiplier 3).
    set_core_item(&mut sim, "copper", 500);
    set_core_item(&mut sim, "lead", 500);
    let sorter = BlockId::new(sim.world_block_id("sorter").expect("sorter"));
    let copper = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .item_id("copper")
        .expect("copper");
    assert!(sim.queue_player_place_config(14, 10, sorter, 0, ConfigValue::Item(copper)));
    tick_n(&mut sim, 120);
    assert_eq!(
        sim.grid.block_at(TilePos::new(14, 10)),
        Some(sorter),
        "sorter finished"
    );
    let entity = sim
        .grid
        .entity_at(TilePos::new(14, 10))
        .expect("sorter entity");
    assert_eq!(
        read_config(&sim.ecs.0, entity),
        ConfigValue::Item(copper),
        "plan config applied on finish"
    );
}

#[test]
fn plan_without_config_keeps_default() {
    use crate::world::config::{ConfigValue, read_config};

    let (mut sim, _unit, _core, wall) = sim_with_player();
    set_core_copper(&mut sim, 100);
    assert!(sim.queue_player_place(14, 10, wall, 0));
    tick_n(&mut sim, 60);
    assert_eq!(sim.grid.block_at(TilePos::new(14, 10)), Some(wall));
    let entity = sim
        .grid
        .entity_at(TilePos::new(14, 10))
        .expect("wall entity");
    assert_eq!(read_config(&sim.ecs.0, entity), ConfigValue::None);
}

/// T2 (PAR-0215): with no player unit, a `player: Some` place in a world
/// whose team has a core changes nothing (Java `Nulls.unit.addBuild` is a
/// no-op; `BuilderComp.addBuild` returns at `if(!canBuild()) return;`).
#[test]
fn player_place_without_unit_is_dropped_when_team_has_core() {
    let mut sim = live_sim_seeded(7, 64);
    let core_id = sim.world_block_id("core-shard").expect("core");
    sim.command(SimCommand::Place {
        x: 10,
        y: 10,
        block: core_id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("core placed");
    let wall = BlockId::new(sim.world_block_id("copper-wall").expect("wall"));
    set_core_copper(&mut sim, 100);
    sim.command(SimCommand::Place {
        x: 20,
        y: 20,
        block: wall.raw(),
        rotation: 0,
        team: 1,
        player: Some(0),
    })
    .expect("dropped");
    tick_n(&mut sim, 120);
    assert_eq!(sim.grid.entity_at(TilePos::new(20, 20)), None);
    assert!(sim.player_build_plans().is_empty(), "no plan queued");
    assert_eq!(core_copper(&sim), 100, "no spend, no refund");
}

/// T2 (PAR-0215): the same for removal — `Break { player: Some(_) }` with
/// no build-capable unit is dropped when the player team owns a core.
#[test]
fn player_break_without_unit_is_dropped_when_team_has_core() {
    let mut sim = live_sim_seeded(7, 64);
    let core_id = sim.world_block_id("core-shard").expect("core");
    let wall = BlockId::new(sim.world_block_id("copper-wall").expect("wall"));
    sim.command(SimCommand::Place {
        x: 10,
        y: 10,
        block: core_id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("core placed");
    sim.command(SimCommand::Place {
        x: 20,
        y: 20,
        block: wall.raw(),
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("wall placed");
    assert_eq!(sim.grid.block_at(TilePos::new(20, 20)), Some(wall));
    sim.command(SimCommand::Break {
        x: 20,
        y: 20,
        player: Some(0),
    })
    .expect("dropped");
    tick_n(&mut sim, 120);
    assert_eq!(
        sim.grid.block_at(TilePos::new(20, 20)),
        Some(wall),
        "wall survives the dead-player break"
    );
}

/// Core-less dev worlds (spine/determinism scenarios) keep the port-only
/// instant placement affordance (no Java counterpart).
#[test]
fn player_place_without_core_stays_instant() {
    let mut sim = live_sim_seeded(7, 64);
    let wall = BlockId::new(sim.world_block_id("copper-wall").expect("wall"));
    sim.command(SimCommand::Place {
        x: 20,
        y: 20,
        block: wall.raw(),
        rotation: 0,
        team: 1,
        player: Some(0),
    })
    .expect("instant");
    sim.tick().expect("tick");
    assert_eq!(sim.grid.block_at(TilePos::new(20, 20)), Some(wall));
}

#[test]
fn rebuild_select_queues_ghost() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    with_campaign(&mut sim);
    set_core_copper(&mut sim, 100);
    let entity = build_wall(&mut sim, 14, 10, wall);
    kill_building(&mut sim, entity);
    assert!(
        team_ghosts(&sim, 1)
            .iter()
            .any(|plan| plan.x == 14 && plan.y == 10),
        "ghost queued"
    );

    assert_eq!(sim.queue_team_rebuild(13, 9, 15, 11), 1);
    let plans = sim.player_build_plans();
    assert_eq!(plans.len(), 1);
    assert_eq!((plans[0].x, plans[0].y, plans[0].block), (14, 10, wall));

    tick_n(&mut sim, 60);
    assert_eq!(
        sim.grid.block_at(TilePos::new(14, 10)),
        Some(wall),
        "rebuilt"
    );
    assert!(
        !team_ghosts(&sim, 1)
            .iter()
            .any(|plan| plan.x == 14 && plan.y == 10),
        "ghost cleared by the rebuild (checkOverlappingPlans)"
    );
}

#[test]
fn rebuild_select_single_tile_rect() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    with_campaign(&mut sim);
    set_core_copper(&mut sim, 100);
    let entity = build_wall(&mut sim, 14, 10, wall);
    kill_building(&mut sim, entity);
    // Zero-width rect still overlaps the block bounds (strict inequalities).
    assert_eq!(sim.queue_team_rebuild(14, 10, 14, 10), 1);
}

#[test]
fn derelict_wall_click_queues_repair() {
    // `InputHandler.tryRepairDerelict` / `canRepairDerelict`
    // (IH:2070-2081): a team-0 derelict wall is queued for the player team
    // when `Build.validPlace` accepts the same block/rotation.
    let (mut sim, _unit, _core, wall) = sim_with_player();
    with_campaign(&mut sim);
    set_core_copper(&mut sim, 100);
    sim.command(SimCommand::Place {
        x: 14,
        y: 10,
        block: wall.raw(),
        rotation: 0,
        team: 0,
        player: None,
    })
    .expect("derelict placed");
    assert_eq!(
        sim.grid
            .entity_at(TilePos::new(14, 10))
            .and_then(|entity| sim.ecs.0.get::<crate::ecs::BuildingComp>(entity))
            .map(|building| building.team.0),
        Some(0),
        "team 0"
    );
    assert!(sim.try_repair_derelict(14, 10));
    let plans = sim.player_build_plans();
    assert_eq!(plans.len(), 1);
    assert_eq!((plans[0].x, plans[0].y, plans[0].block), (14, 10, wall));
    // Own buildings and empty tiles are not derelicts.
    assert!(!sim.try_repair_derelict(10, 10), "core is not derelict");
    assert!(!sim.try_repair_derelict(20, 20), "empty tile");
}

#[test]
fn rebuild_rect_repairs_derelicts() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    with_campaign(&mut sim);
    set_core_copper(&mut sim, 100);
    for (x, y) in [(14, 10), (15, 10), (16, 10)] {
        sim.command(SimCommand::Place {
            x,
            y,
            block: wall.raw(),
            rotation: 0,
            team: 0,
            player: None,
        })
        .expect("derelict placed");
    }
    // `rebuildArea` (IH:2331-2341) queues every derelict in the rect.
    assert_eq!(sim.queue_team_rebuild(13, 9, 17, 11), 3);
    assert_eq!(sim.player_build_plans().len(), 3);
}

#[test]
fn remove_plans_in_cancels_queued_and_ghosts() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    with_campaign(&mut sim);
    // Empty core: the queued plan at (20,20) stays queued.
    assert!(sim.queue_player_place(20, 20, wall, 0));
    assert_eq!(sim.player_build_plans().len(), 1);
    // Ghost at (22,20) from a killed API-placed wall.
    sim.command(SimCommand::Place {
        x: 22,
        y: 20,
        block: wall.raw(),
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("api wall");
    let entity = sim.grid.entity_at(TilePos::new(22, 20)).expect("wall");
    kill_building(&mut sim, entity);
    assert!(
        team_ghosts(&sim, 1)
            .iter()
            .any(|plan| plan.x == 22 && plan.y == 20)
    );

    sim.remove_plans_in(19, 19, 23, 21, 100);
    assert!(sim.player_build_plans().is_empty(), "queued plan removed");
    assert!(
        team_ghosts(&sim, 1).is_empty(),
        "ghost removed from the team queue"
    );
}

#[test]
fn adjacent_block_does_not_clear_ghost() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    with_campaign(&mut sim);
    set_core_copper(&mut sim, 100);
    let entity = build_wall(&mut sim, 14, 10, wall);
    kill_building(&mut sim, entity);
    // A neighbour finishes right next to the ghost: strict bounds do not
    // overlap, so the ghost stays.
    let _ = build_wall(&mut sim, 15, 10, wall);
    assert!(
        team_ghosts(&sim, 1)
            .iter()
            .any(|plan| plan.x == 14 && plan.y == 10),
        "adjacent block leaves the ghost"
    );
}

#[test]
fn out_of_range_plans_wait_for_the_builder() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    set_core_copper(&mut sim, 100);
    // 220 px build range = 27.5 tiles; the unit is at tile (10, 10).
    sim.command(SimCommand::Place {
        x: 50,
        y: 50,
        block: wall.raw(),
        rotation: 0,
        team: 1,
        player: Some(0),
    })
    .expect("queued");
    tick_n(&mut sim, 30);
    assert!(sim.construct_progress(50, 50).is_none());
    assert_eq!(sim.player_build_plans().len(), 1);
}

/// PAR-0098 §2a: `DeletePlans` drops the matching player queue entries
/// (`InputHandler.deletePlans`) and `clear_building` clears the queue.
#[test]
fn delete_plans_removes_player_plan() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    set_core_copper(&mut sim, 100);
    for (x, y) in [(14, 10), (16, 10)] {
        sim.command(SimCommand::Place {
            x,
            y,
            block: wall.raw(),
            rotation: 0,
            team: 1,
            player: Some(0),
        })
        .expect("queued");
    }
    assert_eq!(sim.player_build_plans().len(), 2);
    sim.command(SimCommand::DeletePlans {
        positions: smallvec::smallvec![TilePos::new(14, 10).pack()],
    })
    .expect("delete plans");
    assert_eq!(sim.player_build_plans().len(), 1, "one plan removed");
    assert_eq!(sim.player_build_plans()[0].x, 16);
    sim.clear_player_build_plans();
    assert!(sim.player_build_plans().is_empty(), "queue cleared");
}

/// PAR-0098 §2b: `updateBuilding = false` freezes the queue
/// (`BuilderComp.updateBuildLogic` returns early).
#[test]
fn pausing_building_holds_construct_progress() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    set_core_copper(&mut sim, 100);
    // Pause before the first plan: the queue is created paused and the head
    // never starts a construct block.
    sim.set_player_building_paused(true);
    sim.command(SimCommand::Place {
        x: 14,
        y: 10,
        block: wall.raw(),
        rotation: 0,
        team: 1,
        player: Some(0),
    })
    .expect("queued");
    tick_n(&mut sim, 30);
    assert!(
        sim.construct_progress(14, 10).is_none(),
        "no construction while paused"
    );
    assert_eq!(sim.player_build_plans().len(), 1, "plan waits");
    // Resume: the plan completes.
    sim.set_player_building_paused(false);
    tick_n(&mut sim, 60);
    assert_eq!(
        sim.grid.block_at(TilePos::new(14, 10)),
        Some(wall),
        "resumed and finished"
    );
}

/// PAR-0098 §2c: a ground unit standing on the plan's footprint rotates the
/// plan to the back of the queue (`Build.checkNoUnitOverlap`); once it
/// moves away the plan proceeds.
#[test]
fn unit_overlap_delays_construction() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    set_core_copper(&mut sim, 100);
    // A ground (`flying == false`) unit standing on the target tile.
    let runtime = sim.block_runtime().expect("runtime");
    let dagger = runtime.content().unit_id("dagger").expect("dagger");
    let def = runtime.content().unit(dagger).cloned().expect("dagger def");
    let blocker = crate::entities::comp::unit::lifecycle::spawn_unit_def(
        &mut sim.ecs.0,
        42,
        &def,
        1,
        14.0 * 8.0,
        10.0 * 8.0,
        90.0,
    );
    // Keep it in place (`CommandAI` with no target idles).
    if let Some(mut slot) = sim
        .ecs
        .0
        .get_mut::<crate::ai::controller::ControllerSlot>(blocker)
    {
        slot.kind = crate::ai::controller::AiKind::Command;
        slot.target = None;
    }
    sim.command(SimCommand::Place {
        x: 14,
        y: 10,
        block: wall.raw(),
        rotation: 0,
        team: 1,
        player: Some(0),
    })
    .expect("queued");
    tick_n(&mut sim, 30);
    assert!(
        sim.construct_progress(14, 10).is_none(),
        "unit blocks the plan"
    );
    assert_eq!(sim.player_build_plans().len(), 1, "plan waits");

    // Move the unit away: the plan starts and completes.
    if let Some(mut pos) = sim.ecs.0.get_mut::<Pos>(blocker) {
        pos.x = 20.0 * 8.0;
        pos.y = 20.0 * 8.0;
    }
    tick_n(&mut sim, 120);
    assert_eq!(
        sim.grid.block_at(TilePos::new(14, 10)),
        Some(wall),
        "built after the unit moved"
    );
}

/// T6 (`Units.java:29-35`): a legged unit (`atrax`, `allowLegStep`) on the
/// plan's footprint does not block construction.
#[test]
fn legged_unit_does_not_block_construction() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    set_core_copper(&mut sim, 100);
    let runtime = sim.block_runtime().expect("runtime");
    let atrax = runtime.content().unit_id("atrax").expect("atrax");
    let def = runtime.content().unit(atrax).cloned().expect("atrax def");
    assert!(def.allow_leg_step, "atrax is a legged unit");
    let blocker = crate::entities::comp::unit::lifecycle::spawn_unit_def(
        &mut sim.ecs.0,
        43,
        &def,
        1,
        14.0 * 8.0,
        10.0 * 8.0,
        90.0,
    );
    if let Some(mut slot) = sim
        .ecs
        .0
        .get_mut::<crate::ai::controller::ControllerSlot>(blocker)
    {
        slot.kind = crate::ai::controller::AiKind::Command;
        slot.target = None;
    }
    sim.command(SimCommand::Place {
        x: 14,
        y: 10,
        block: wall.raw(),
        rotation: 0,
        team: 1,
        player: Some(0),
    })
    .expect("queued");
    tick_n(&mut sim, 60);
    assert_eq!(
        sim.grid.block_at(TilePos::new(14, 10)),
        Some(wall),
        "the legged unit does not block the wall"
    );
}

#[test]
fn no_items_means_no_construction() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    set_core_copper(&mut sim, 0);
    sim.command(SimCommand::Place {
        x: 14,
        y: 10,
        block: wall.raw(),
        rotation: 0,
        team: 1,
        player: Some(0),
    })
    .expect("queued");
    tick_n(&mut sim, 30);
    assert_ne!(sim.grid.block_at(TilePos::new(14, 10)), Some(wall));
    assert_eq!(sim.player_build_plans().len(), 1, "plan stays queued");
}

/// T11 (`ItemBridge.playerPlaced`, `ItemBridge.java:224-233`): two
/// player-built bridge-conveyors 3 tiles apart chain on construction.
#[test]
fn bridges_chain_when_player_placed() {
    let (mut sim, _unit, _core, _wall) = sim_with_player();
    set_core_copper(&mut sim, 10_000);
    // bridge-conveyor needs lead 6 + copper 6 (`registries/blocks/distribution.rs`).
    let lead = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .item_id("lead")
        .expect("lead");
    let core = team_core(&sim.ecs.0, 1).expect("core");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemModule>(core) {
        items.add(lead, 1_000, 100_000);
    }
    let bridge = BlockId::new(
        sim.world_block_id("bridge-conveyor")
            .expect("bridge-conveyor"),
    );
    assert!(
        sim.queue_player_place(20, 20, bridge, 0),
        "first bridge queued"
    );
    assert!(
        sim.queue_player_place(23, 20, bridge, 0),
        "second bridge queued"
    );
    for _ in 0..600 {
        sim.tick().expect("tick");
        let finished = sim
            .grid
            .entity_at(TilePos::new(23, 20))
            .is_some_and(|entity| sim.ecs.0.get::<ItemBridgeBuild>(entity).is_some());
        if finished {
            break;
        }
    }
    let first = sim
        .grid
        .entity_at(TilePos::new(20, 20))
        .expect("first bridge built");
    let second = sim
        .grid
        .entity_at(TilePos::new(23, 20))
        .expect("second bridge built");
    let link = sim
        .ecs
        .0
        .get::<ItemBridgeBuild>(first)
        .map(|bridge| bridge.link)
        .unwrap_or(-1);
    assert_eq!(
        link,
        TilePos::new(23, 20).pack(),
        "the first bridge links to the second"
    );
    let second_link = sim
        .ecs
        .0
        .get::<ItemBridgeBuild>(second)
        .map(|bridge| bridge.link)
        .unwrap_or(-1);
    assert_eq!(second_link, -1, "the second waits for a later bridge");
}

/// `HintsFragment.java:74-102`: player placements and breaks feed the hint
/// sets (`placedBlocks` / `"break"`).
#[test]
fn hint_events_record_player_placements() {
    // Player-queued placement (`BlockBuildEndEvent` with `!breaking`).
    let (mut sim, _unit, _core, wall) = sim_with_player();
    set_core_copper(&mut sim, 100);
    assert!(sim.queue_player_place(14, 10, wall, 0), "plan queued");
    for _ in 0..120 {
        sim.tick().expect("tick");
        if sim.hint_events.placed_by_player.contains(&wall) {
            break;
        }
    }
    assert!(
        sim.hint_events.placed_by_player.contains(&wall),
        "the player wall placement is recorded"
    );

    // Player-queued deconstruction of an instant-placed wall fires "break".
    let (mut sim, _unit, _core, wall) = sim_with_player();
    sim.command(SimCommand::Place {
        x: 14,
        y: 10,
        block: wall.raw(),
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("instant API place");
    // The local player is team 1 (`sharded`); sync the guard like a
    // campaign load does (`sync_build_rules`).
    sim.ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("build rules")
        .default_team = 1;
    sim.command(SimCommand::Break {
        x: 14,
        y: 10,
        player: Some(0),
    })
    .expect("queued break");
    for _ in 0..120 {
        sim.tick().expect("tick");
        if sim.hint_events.events.contains("break") {
            break;
        }
    }
    assert!(
        sim.hint_events.events.contains("break"),
        "the break event is recorded"
    );
}

/// `Trigger.fireCreate` (G05 T4b): a fire born during a tick arms the
/// `waveFire` hint event.
#[test]
fn fire_creation_records_hint_event() {
    let (mut sim, _unit, _core, _wall) = sim_with_player();
    let mut rng = crate::determinism::SimRng::new(3);
    assert!(crate::combat::fires::create(&mut sim.ecs.0, 5, 5, &mut rng).is_some());
    sim.tick().expect("tick");
    assert!(sim.hint_events.events.contains("fire"));
}

/// T18: a queued player placement keeps its rotation through the plan queue
/// and the finished block.
#[test]
fn queue_player_place_keeps_rotation() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    set_core_copper(&mut sim, 100);
    assert!(sim.queue_player_place(14, 10, wall, 1), "plan queued");
    let plans = sim.player_build_plans();
    assert_eq!(plans.first().map(|plan| plan.rotation), Some(1));
    tick_n(&mut sim, 60);
    let entity = sim
        .grid
        .entity_at(TilePos::new(14, 10))
        .expect("wall built");
    assert_eq!(
        sim.ecs
            .0
            .get::<Building>(entity)
            .map(|building| building.rotation),
        Some(1),
        "built block keeps the plan rotation"
    );
}

#[test]
fn player_break_deconstructs_and_refunds() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    sim.command(SimCommand::Place {
        x: 14,
        y: 10,
        block: wall.raw(),
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("instant API place");
    assert_eq!(sim.grid.block_at(TilePos::new(14, 10)), Some(wall));
    set_core_copper(&mut sim, 10);
    // The local player is team 1 (`sharded`); sync the guard like a
    // campaign load does (`sync_build_rules`).
    sim.ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("build rules")
        .default_team = 1;
    sim.command(SimCommand::Break {
        x: 14,
        y: 10,
        player: Some(0),
    })
    .expect("queued break");
    assert_eq!(
        sim.grid.block_at(TilePos::new(14, 10)),
        Some(wall),
        "not instant"
    );
    tick_n(&mut sim, 60);
    assert_eq!(sim.grid.block_at(TilePos::new(14, 10)), Some(BlockId::AIR));
    // 6 copper * 0.5 refund.
    assert_eq!(core_copper(&sim), 13);
}

/// G03 T22: finishing a construct emits `PLACE_BLOCK`, deconstructing
/// `BREAK_BLOCK` (rotation argument = the block size).
#[test]
fn construct_finish_emits_place_fx() {
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingSink(Mutex<Vec<(crate::content::EffectId, f32, f32, f32)>>);

    impl crate::combat::view::FxSink for RecordingSink {
        fn effect(
            &self,
            effect: &crate::content::registries::fx_meta::EffectRef,
            x: f32,
            y: f32,
            rotation: f32,
            _color: crate::content::Rgba,
        ) {
            if let crate::content::registries::fx_meta::EffectRef::Named(id) = effect {
                self.0.lock().expect("lock").push((*id, x, y, rotation));
            }
        }
    }

    let (mut sim, _unit, _core, wall) = sim_with_player();
    let sink = Arc::new(RecordingSink::default());
    if let Some(mut runtime) = sim.unit_runtime_mut() {
        runtime.fx = sink.clone();
    }
    set_core_copper(&mut sim, 100);
    // The local player is team 1 (`sharded`); sync the guard like a
    // campaign load does (`sync_build_rules`).
    sim.ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("build rules")
        .default_team = 1;

    assert!(sim.queue_player_place(20, 20, wall, 1), "wall queued");
    tick_n(&mut sim, 120);
    {
        let events = sink.0.lock().expect("lock");
        let place = events
            .iter()
            .find(|(id, ..)| *id == crate::content::EffectId::PLACE_BLOCK)
            .unwrap_or_else(|| panic!("PLACE_BLOCK missing: {events:?}"));
        assert!((place.1 - 160.0).abs() < 1e-3, "cx: {place:?}");
        assert!((place.2 - 160.0).abs() < 1e-3, "cy: {place:?}");
        assert!((place.3 - 1.0).abs() < 1e-3, "rotation = size: {place:?}");
    }
    assert!(
        sim.command(SimCommand::Break {
            x: 20,
            y: 20,
            player: Some(0),
        }) == Ok(())
    );
    tick_n(&mut sim, 120);
    let events = sink.0.lock().expect("lock");
    let count = events
        .iter()
        .filter(|(id, ..)| *id == crate::content::EffectId::BREAK_BLOCK)
        .count();
    assert!(count >= 1, "BREAK_BLOCK missing: {events:?}");
}

/// G03 T2: the preview validity check agrees with the queued placement.
#[test]
fn preview_can_place_matches_queue() {
    let (mut sim, _unit, _core, wall) = sim_with_player();
    sim.tick().expect("tick");
    assert!(
        sim.preview_can_place(20, 20, wall, 0),
        "empty tile is placeable in preview"
    );
    assert!(
        !sim.preview_can_place(10, 10, wall, 0),
        "the core footprint is not placeable"
    );
    assert!(
        sim.queue_player_place(20, 20, wall, 0),
        "the queued placement agrees"
    );
    assert!(
        !sim.queue_player_place(10, 10, wall, 0),
        "the queued placement rejects the core tile"
    );
}

/// H02 T4: `CoreBlock.placeBegan` (`CoreBlock.java:205-239`): upgrading a
/// shard to a foundation over the same tile is instant and keeps the old
/// items minus the requirements.
#[test]
fn core_upgrade_keeps_items() {
    let (mut sim, _unit, _core, _wall) = sim_with_player();
    let foundation = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("core-foundation")
        .expect("core-foundation");
    // Shard capacity 4000 per item (`Blocks.java:3152`); foundation costs
    // copper 3000 / lead 3000 / silicon 2000 (`Blocks.java:3160`).
    let (copper, lead, silicon) = {
        let content = &sim.block_runtime().expect("runtime").content();
        (
            content.item_id("copper").expect("copper"),
            content.item_id("lead").expect("lead"),
            content.item_id("silicon").expect("silicon"),
        )
    };
    {
        let core = team_core(&sim.ecs.0, 1).expect("core");
        let mut items = sim.ecs.0.get_mut::<ItemModule>(core).expect("items");
        for item in [copper, lead, silicon] {
            items.add(item, 4000, 4000);
        }
    }
    assert!(
        sim.queue_player_place(10, 10, foundation, 0),
        "upgrade plan queued"
    );
    for _ in 0..600 {
        sim.tick().expect("tick");
        if sim.player_build_plans().is_empty() {
            break;
        }
    }
    assert!(
        sim.player_build_plans().is_empty(),
        "the upgrade plan finished"
    );
    assert_eq!(
        sim.grid.block_at(TilePos::new(10, 10)),
        Some(foundation),
        "core upgraded instantly"
    );
    let core = team_core(&sim.ecs.0, 1).expect("core");
    let items = sim.ecs.0.get::<ItemModule>(core).expect("items");
    assert_eq!(items.get(copper), 1000, "copper kept minus cost");
    assert_eq!(items.get(lead), 1000);
    assert_eq!(items.get(silicon), 2000);
}

/// H02 T4: a core can only be placed over a smaller own core
/// (`CoreBlock.canPlaceOn`, `CoreBlock.java:185-203`).
#[test]
fn core_needs_existing_core() {
    let (sim, _unit, _core, _wall) = sim_with_player();
    let foundation = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("core-foundation")
        .expect("core-foundation");
    assert!(
        !sim.valid_place_player(30, 30, foundation, 0),
        "foundation on free ground rejected"
    );
    // Over the shard without the requirements: rejected (`core == null` or
    // `!items.has(requirements)`).
    assert!(
        !sim.valid_place_player(10, 10, foundation, 0),
        "upgrade without items rejected"
    );
}

/// H02 T4: the upgrade plan needs every requirement in the team core
/// (`CoreBlock.canPlaceOn`).
#[test]
fn core_upgrade_needs_items() {
    let (mut sim, _unit, _core, _wall) = sim_with_player();
    let foundation = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("core-foundation")
        .expect("core-foundation");
    let copper = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .item_id("copper")
        .expect("copper");
    {
        let core = team_core(&sim.ecs.0, 1).expect("core");
        let mut items = sim.ecs.0.get_mut::<ItemModule>(core).expect("items");
        items.add(copper, 100, 4000);
    }
    assert!(
        !sim.valid_place_player(10, 10, foundation, 0),
        "copper 100 < 3000"
    );
    let (lead, silicon) = {
        let content = &sim.block_runtime().expect("runtime").content();
        (
            content.item_id("lead").expect("lead"),
            content.item_id("silicon").expect("silicon"),
        )
    };
    {
        let core = team_core(&sim.ecs.0, 1).expect("core");
        let mut items = sim.ecs.0.get_mut::<ItemModule>(core).expect("items");
        items.add(copper, 3000, 4000);
        items.add(lead, 3000, 4000);
        items.add(silicon, 2000, 4000);
    }
    assert!(
        sim.valid_place_player(10, 10, foundation, 0),
        "all requirements present"
    );
}

/// H08 T2: the placement-preview validity is the exact
/// `queue_player_place_config` check (`Build.validPlace` under
/// `PlaceFlags::PLAYER`; `MindSimHost::can_place_player` forwards here).
/// The seam is H02 T3's `Sim::valid_place_player`.
#[test]
fn can_place_player_matches_queue() {
    let (sim, _unit, _core, wall) = sim_with_player();
    assert!(
        sim.valid_place_player(14, 10, wall, 0),
        "free ground accepts a copper wall"
    );
    // The core occupies (10,10); the same gate `queue_player_place_config`
    // runs rejects the wall there.
    assert!(
        !sim.valid_place_player(10, 10, wall, 0),
        "core tile rejects a copper wall"
    );
}

/// H02 T6: `ItemBridge.playerPlaced` chaining (`chain_bridge`) links the
/// previously placed same-block bridge, skips one already in the new
/// bridge's proximity, and covers `LiquidBridge` (the pre-T6 comment
/// claimed Java's liquid bridge does not chain; `LiquidBridge.java:7,19`
/// inherits `playerPlaced`).
#[test]
fn player_bridge_chaining_links_and_skips_proximity() {
    fn bridge_link(sim: &Sim, e: Entity) -> i32 {
        sim.ecs
            .0
            .get::<ItemBridgeBuild>(e)
            .map(|bridge| bridge.link)
            .unwrap_or(-2)
    }

    let mut sim = Sim::new(7, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    let place = |sim: &mut Sim, name: &str, x: i16, y: i16| -> (Entity, BlockId) {
        let block = BlockId::new(sim.world_block_id(name).expect("block"));
        sim.command(SimCommand::Place {
            x,
            y,
            block: block.raw(),
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("placed");
        (
            sim.grid.entity_at(TilePos::new(x, y)).expect("entity"),
            block,
        )
    };

    // Far apart: the second bridge links back to the first.
    let (a, block) = place(&mut sim, "bridge-conveyor", 5, 5);
    let (b, _) = place(&mut sim, "bridge-conveyor", 8, 5);
    let mut plans = BuilderPlans {
        last_bridge: Some((block, TilePos::new(5, 5))),
        ..BuilderPlans::default()
    };
    let runtime = sim.block_runtime.take().expect("runtime");
    chain_bridge(
        &mut sim.ecs.0,
        &sim.grid,
        &runtime,
        b,
        &mut plans,
        block,
        TilePos::new(8, 5),
    );
    sim.block_runtime = Some(runtime);
    assert_eq!(bridge_link(&sim, a), TilePos::new(8, 5).pack());

    // Adjacent (`!proximity.contains(link.build)`): no link.
    let (c, block) = place(&mut sim, "bridge-conveyor", 12, 5);
    let (d, _) = place(&mut sim, "bridge-conveyor", 13, 5);
    plans.last_bridge = Some((block, TilePos::new(12, 5)));
    let runtime = sim.block_runtime.take().expect("runtime");
    chain_bridge(
        &mut sim.ecs.0,
        &sim.grid,
        &runtime,
        d,
        &mut plans,
        block,
        TilePos::new(13, 5),
    );
    sim.block_runtime = Some(runtime);
    assert_eq!(bridge_link(&sim, c), -1, "proximity blocks chaining");

    // `LiquidBridge extends ItemBridge` and chains like a bridge.
    let (e, block) = place(&mut sim, "bridge-conduit", 5, 9);
    let (f, _) = place(&mut sim, "bridge-conduit", 8, 9);
    plans.last_bridge = Some((block, TilePos::new(5, 9)));
    let runtime = sim.block_runtime.take().expect("runtime");
    chain_bridge(
        &mut sim.ecs.0,
        &sim.grid,
        &runtime,
        f,
        &mut plans,
        block,
        TilePos::new(8, 9),
    );
    sim.block_runtime = Some(runtime);
    assert_eq!(bridge_link(&sim, e), TilePos::new(8, 9).pack());
}

/// GAPS #62: `ConstructBlock.constructFinish` calls
/// `Block.placeEnded(tile, builder, rotation, config)` after `configure` and
/// the place effect (`ConstructBlock.java:72-118`).
#[test]
fn construct_finish_calls_place_ended() {
    use std::sync::Arc;

    #[derive(Debug, Clone, Default, PartialEq)]
    struct PlaceEndedStamp {
        entity: Option<Entity>,
        builder: Option<Entity>,
        rotation: u8,
        config: crate::world::config::ConfigValue,
    }

    #[derive(bevy_ecs::prelude::Resource, Default)]
    struct PlaceEndedLog(Vec<PlaceEndedStamp>);

    struct ProbeBehavior;

    impl crate::world::behavior::BuildingBehavior for ProbeBehavior {
        fn place_ended(
            &self,
            world: &mut World,
            e: Entity,
            builder: Option<Entity>,
            rotation: u8,
            config: &crate::world::config::ConfigValue,
        ) {
            let stamp = PlaceEndedStamp {
                entity: Some(e),
                builder,
                rotation,
                config: config.clone(),
            };
            if world.get_resource::<PlaceEndedLog>().is_none() {
                world.insert_resource(PlaceEndedLog::default());
            }
            if let Some(mut log) = world.get_resource_mut::<PlaceEndedLog>() {
                log.0.push(stamp);
            }
        }
    }

    let (mut sim, unit, _core, wall) = sim_with_player();
    // Rebuild the block table with a probe behavior on the wall so the
    // default wall behavior cannot swallow the hook.
    let content = sim.block_runtime().expect("runtime").content().clone();
    let mut registry = crate::world::blocks::default_registry(&content);
    registry.register_named("copper-wall", Arc::new(ProbeBehavior));
    let table = BlockTable::build(&content, &registry).expect("block table");
    sim.ecs.0.insert_resource(table);
    sim.ecs.0.insert_resource(PlaceEndedLog::default());

    set_core_copper(&mut sim, 100);
    let copper = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .item_id("copper")
        .expect("copper");
    assert!(
        sim.queue_player_place_config(
            20,
            20,
            wall,
            2,
            crate::world::config::ConfigValue::Item(copper),
        ),
        "wall queued"
    );
    tick_n(&mut sim, 120);
    let placed = sim
        .grid
        .entity_at(TilePos::new(20, 20))
        .expect("wall placed");
    let log = sim
        .ecs
        .0
        .get_resource::<PlaceEndedLog>()
        .expect("place_ended log");
    let stamp = log.0.last().expect("place_ended called");
    assert_eq!(stamp.entity, Some(placed));
    assert_eq!(stamp.builder, Some(unit), "builder unit passed through");
    assert_eq!(stamp.rotation, 2, "plan rotation passed through");
    assert_eq!(
        stamp.config,
        crate::world::config::ConfigValue::Item(copper),
        "plan config passed through"
    );
}
