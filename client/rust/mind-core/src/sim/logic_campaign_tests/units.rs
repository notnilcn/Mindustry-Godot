// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! I03 T1/T2 unit control (ubind/ucontrol/ulocate) and objective-completion scripts.
//! Upstream: core/src/mindustry/logic/LExecutor.java.

use super::world_effects::{processor_num, processor_obj};
use super::*;
use crate::sim::test_support::campaign_runtime_mut;

// ---- I03 T1: ubind/ucontrol/ulocate drive live units (PAR-0221) ----

/// Reads the processor's bound `@unit`.
fn processor_bound_unit(sim: &Sim, x: i16, y: i16) -> Option<Entity> {
    let e = sim.grid.entity_at(TilePos::new(x, y)).expect("processor");
    let state = sim.ecs.0.get::<LogicBlockState>(e).expect("state");
    match state.executor.arena.get(state.executor.unit).value_obj() {
        Some(LogicObject::Unit(unit)) => Some(*unit),
        _ => None,
    }
}

fn controller_kind(sim: &Sim, e: Entity) -> crate::ai::AiKind {
    sim.ecs
        .0
        .get::<crate::ai::ControllerSlot>(e)
        .expect("controller slot")
        .kind
}

pub(super) fn entity_pos(sim: &Sim, e: Entity) -> (f32, f32) {
    let pos = sim.ecs.0.get::<crate::entities::comp::Pos>(e).expect("pos");
    (pos.x, pos.y)
}

fn mono_stack(sim: &Sim, e: Entity) -> Option<(crate::content::ItemId, i32)> {
    sim.ecs
        .0
        .get::<crate::entities::comp::unit::comp::ItemsComp>(e)
        .and_then(|items| items.item)
}

fn container_count(sim: &Sim, e: Entity, item: crate::content::ItemId) -> i32 {
    sim.ecs
        .0
        .get::<crate::world::modules::ItemModule>(e)
        .map(|module| module.get(item))
        .unwrap_or(0)
}

/// `ubind @poly` cycles the team's polys in spawn order (`%=`/`++`).
#[test]
fn ubind_type_cycles_in_spawn_order() {
    let mut sim = campaign_sim();
    let p1 = sim
        .spawn_live_unit("poly", 1, 80.0, 80.0, 0.0)
        .expect("poly 1");
    let p2 = sim
        .spawn_live_unit("poly", 1, 120.0, 80.0, 0.0)
        .expect("poly 2");
    let p3 = sim
        .spawn_live_unit("poly", 1, 160.0, 80.0, 0.0)
        .expect("poly 3");
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @poly\nset a @unit\nubind @poly\nset b @unit\nubind @poly\nset c @unit\nubind @poly\nset d @unit\nstop\n",
    );
    tick_n(&mut sim, 10);
    assert_eq!(
        processor_obj(&sim, 10, 10, "a"),
        Some(LogicObject::Unit(p1))
    );
    assert_eq!(
        processor_obj(&sim, 10, 10, "b"),
        Some(LogicObject::Unit(p2))
    );
    assert_eq!(
        processor_obj(&sim, 10, 10, "c"),
        Some(LogicObject::Unit(p3))
    );
    assert_eq!(
        processor_obj(&sim, 10, 10, "d"),
        Some(LogicObject::Unit(p1)),
        "the cursor wraps"
    );
    assert_events_drained(&sim);
}

/// `ubind <unit object>` binds that unit (`UnitBindI` object branch).
#[test]
fn ubind_unit_object() {
    let mut sim = campaign_sim();
    place_team1(&mut sim, "core-shard", 20, 20);
    place_world_processor(
        &mut sim,
        10,
        10,
        "spawn @dagger 14 14 0 @sharded u\nubind u\nset b @unit\nstop\n",
    );
    tick_n(&mut sim, 10);
    let spawned = processor_obj(&sim, 10, 10, "u").expect("spawn result");
    assert!(matches!(spawned, LogicObject::Unit(_)));
    assert_eq!(processor_obj(&sim, 10, 10, "b"), Some(spawned));
    assert_events_drained(&sim);
}

/// A non-privileged micro processor with `logicUnitControl = false` binds
/// nothing (`UnitBindI` gate).
#[test]
fn ubind_gate() {
    let mut rules = test_rules();
    rules.logic_unit_control = false;
    let mut sim = finish_sim(runtime_sim(), rules);
    let poly = sim
        .spawn_live_unit("poly", 1, 80.0, 80.0, 0.0)
        .expect("poly");
    place_team1(&mut sim, "micro-processor", 12, 12);
    let bytes =
        crate::logic::blocks::logic_block::compress("ubind @poly\nset a @unit\nstop\n", &[]);
    configure(
        &mut sim,
        12,
        12,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    tick_n(&mut sim, 10);
    assert_eq!(
        processor_bound_unit(&sim, 12, 12),
        None,
        "gate leaves @unit null"
    );
    let _ = poly;
    assert_events_drained(&sim);
}

/// `ucontrol move` swaps the controller to LogicAI and walks to the target.
#[test]
fn ucontrol_move_reaches_target() {
    let mut sim = campaign_sim();
    place_team1(&mut sim, "core-shard", 20, 20);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 40.0, 40.0, 0.0)
        .expect("dagger");
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @dagger\nucontrol move 10 10 0 0 0\n",
    );
    tick_n(&mut sim, 300);
    let (x, y) = entity_pos(&sim, dagger);
    let (dx, dy) = (x - 80.0, y - 80.0);
    assert!(
        dx * dx + dy * dy <= 16.0 * 16.0,
        "dagger within 16 px of (80,80): ({x},{y})"
    );
    assert_eq!(controller_kind(&sim, dagger), crate::ai::AiKind::Logic);
    assert_events_drained(&sim);
}

/// A unit with a live CommandAI move order is not logic-controllable
/// (`CommandAI.isLogicControllable` = `!hasCommand()`).
#[test]
fn commanded_unit_not_logic_controllable() {
    let mut sim = campaign_sim();
    place_team1(&mut sim, "core-shard", 20, 20);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 40.0, 40.0, 0.0)
        .expect("dagger");
    {
        let mut slot = sim
            .ecs
            .0
            .get_mut::<crate::ai::ControllerSlot>(dagger)
            .expect("slot");
        slot.kind = crate::ai::AiKind::Command;
        slot.target = Some(TilePos::new(30, 30));
    }
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @dagger\nucontrol move 10 10 0 0 0\n",
    );
    tick_n(&mut sim, 20);
    assert_eq!(
        controller_kind(&sim, dagger),
        crate::ai::AiKind::Command,
        "the command order keeps CommandAI"
    );
    assert!(
        sim.ecs
            .0
            .get::<crate::ai::types::logic::LogicAi>(dagger)
            .is_none(),
        "no LogicAI component is installed"
    );
    assert_events_drained(&sim);
}

/// 600 ticks after the processor stops issuing unit commands, the unit
/// returns to its type's default controller (`LogicAI.controlTimer`).
#[test]
fn logic_ai_times_out() {
    let mut sim = campaign_sim();
    place_team1(&mut sim, "core-shard", 20, 20);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 40.0, 40.0, 0.0)
        .expect("dagger");
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @dagger\nucontrol move 10 10 0 0 0\n",
    );
    tick_n(&mut sim, 30);
    assert_eq!(controller_kind(&sim, dagger), crate::ai::AiKind::Logic);
    // Replace the code with `stop`: no further ucontrol for 601 ticks.
    let bytes = crate::logic::blocks::logic_block::compress("stop\n", &[]);
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    tick_n(&mut sim, 601);
    assert_eq!(
        controller_kind(&sim, dagger),
        crate::ai::AiKind::Command,
        "the campaign default for a player-team dagger"
    );
    assert!(
        sim.ecs
            .0
            .get::<crate::ai::types::logic::LogicAi>(dagger)
            .is_none(),
        "LogicAi is removed on timeout"
    );
    assert_events_drained(&sim);
}

/// `ucontrol within` writes p4; `ucontrol getBlock` writes block/building/
/// floor.
#[test]
fn ucontrol_within_and_getblock() {
    let mut sim = campaign_sim();
    place_team1(&mut sim, "core-shard", 20, 20);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @dagger\nucontrol within 12 12 2 r\nucontrol getBlock 10 10 t b f\nstop\n",
    );
    tick_n(&mut sim, 5);
    assert_eq!(processor_num(&sim, 10, 10, "r"), Some(1.0), "inside 16 px");
    let processor = sim.grid.entity_at(TilePos::new(10, 10)).expect("processor");
    let content = sim.block_runtime().expect("runtime").content();
    let world_processor = content.block_id("world-processor").expect("block");
    let stone = content.block_id("stone").expect("stone");
    assert_eq!(
        processor_obj(&sim, 10, 10, "t"),
        Some(LogicObject::Content(crate::content::ContentRef::block(
            world_processor
        ))),
        "t is the block at (10,10)"
    );
    assert_eq!(
        processor_obj(&sim, 10, 10, "b"),
        Some(LogicObject::Building(processor)),
        "b is the building at (10,10)"
    );
    assert_eq!(
        processor_obj(&sim, 10, 10, "f"),
        Some(LogicObject::Content(crate::content::ContentRef::block(
            stone
        ))),
        "f is the floor"
    );
    let _ = dagger;
    assert_events_drained(&sim);
}

/// `ucontrol itemTake` pulls items into the unit stack; a second call within
/// the 90-tick `transferDelay` does nothing; `ucontrol itemDrop` puts them
/// back into the container (`BuildingComp.acceptStack`).
#[test]
fn ucontrol_item_take_and_drop() {
    let mut sim = campaign_sim();
    place_team1(&mut sim, "core-shard", 20, 20);
    let container = place_team1(&mut sim, "container", 12, 12);
    let copper = item(&sim, "copper");
    sim.ecs
        .0
        .get_mut::<crate::world::modules::ItemModule>(container)
        .expect("items")
        .add(copper, 20, 100);
    let mono = sim
        .spawn_live_unit("mono", 1, 96.0, 96.0, 0.0)
        .expect("mono");
    let links = [LogicLink::new("container1", 2, 2)];
    place_linked_world_processor(
        &mut sim,
        10,
        10,
        "ubind @mono\nucontrol itemTake container1 @copper 10\n",
        &links,
    );
    // First transfer happens at the first tick past the 90-tick delay.
    tick_n(&mut sim, 100);
    assert_eq!(mono_stack(&sim, mono), Some((copper, 10)));
    assert_eq!(container_count(&sim, container, copper), 10);
    // Still inside the delay for the second call.
    tick_n(&mut sim, 70);
    assert_eq!(mono_stack(&sim, mono), Some((copper, 10)));
    // Past it: the take runs again.
    tick_n(&mut sim, 20);
    assert_eq!(mono_stack(&sim, mono), Some((copper, 20)));
    assert_eq!(container_count(&sim, container, copper), 0);
    // Drop half of it back.
    let bytes = crate::logic::blocks::logic_block::compress(
        "ubind @mono\nucontrol itemDrop container1 10\n",
        &links,
    );
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    tick_n(&mut sim, 100);
    assert_eq!(mono_stack(&sim, mono), Some((copper, 10)));
    assert_eq!(container_count(&sim, container, copper), 10);
    assert_events_drained(&sim);
}

/// `ucontrol build` queues a plan the builder executes (`logicUnitBuild`).
#[test]
fn ucontrol_build_places_plan() {
    let mut rules = test_rules();
    rules.logic_unit_build = true;
    rules.instant_build = true;
    rules.infinite_resources = true;
    let mut sim = finish_sim(wide_sim(64), rules);
    place_team1(&mut sim, "core-shard", 20, 20);
    let poly = sim
        .spawn_live_unit("poly", 1, 96.0, 112.0, 0.0)
        .expect("poly");
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @poly\nucontrol build 12 12 @copper-wall 0 0\nstop\n",
    );
    tick_n(&mut sim, 200);
    let wall = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("copper-wall")
        .expect("wall");
    assert_eq!(
        sim.grid.block_at(TilePos::new(12, 12)),
        Some(wall),
        "the poly placed the wall at (12,12)"
    );
    assert_eq!(controller_kind(&sim, poly), crate::ai::AiKind::Logic);
    assert_events_drained(&sim);
}

/// `ulocate building core` returns the own core's tile, `f == 1` and the
/// building object.
#[test]
fn ulocate_building_core() {
    let mut sim = campaign_sim();
    let core = place_team1(&mut sim, "core-shard", 20, 20);
    let poly = sim
        .spawn_live_unit("poly", 1, 100.0, 100.0, 0.0)
        .expect("poly");
    place_world_processor(
        &mut sim,
        10,
        10,
        "ubind @poly\nulocate building core false @copper x y f b\nstop\n",
    );
    tick_n(&mut sim, 5);
    assert_eq!(processor_num(&sim, 10, 10, "f"), Some(1.0), "found");
    let (cx, cy) = entity_pos(&sim, core);
    assert_eq!(
        processor_num(&sim, 10, 10, "x"),
        Some((cx / 8.0) as f64),
        "outX is the core's tile x"
    );
    assert_eq!(processor_num(&sim, 10, 10, "y"), Some((cy / 8.0) as f64));
    assert_eq!(
        processor_obj(&sim, 10, 10, "b"),
        Some(LogicObject::Building(core))
    );
    let _ = poly;
    assert_events_drained(&sim);
}

// ---- I03 T2: objective completionLogicCode runs live (PAR-0224) ----

/// An objective whose `completionLogicCode` sets a flag runs against the live
/// world when it completes (`MapObjectives.java:213` ->
/// `LExecutor.runLogicScript`).
#[test]
fn objective_completion_script_runs_live() {
    use crate::io::json::objectives::{FlagObjective, MapObjective};
    let mut sim = campaign_sim();
    let mut flag = FlagObjective {
        flag: "x".to_owned(),
        ..Default::default()
    };
    flag.common.completion_logic_code = Some("setflag \"x\" true\nstop".to_owned());
    let data = crate::io::json::objectives::MapObjectives {
        all: vec![MapObjective::Flag(flag)],
    };
    {
        let mut runtime = campaign_runtime_mut(&mut sim);
        runtime.session.objectives =
            crate::game::map_objectives::MapObjectivesRuntime::from_data(data);
        let session = &mut runtime.session;
        assert!(
            session.objectives.complete(0, &mut session.rules),
            "objective completes"
        );
    }
    assert!(!has_flag(&sim, "x"), "not yet run");
    tick_n(&mut sim, 1);
    assert!(
        has_flag(&sim, "x"),
        "the completion script set the live flag"
    );
    assert_events_drained(&sim);
}

/// I01 T6: the `explosion` instruction passes its own air/ground flags
/// (`LStatements.ExplosionI` -> `Damage.damage(..., air, ground, ...)`,
/// Damage.java:509-511), so an air-only explosion spares grounded units.
#[test]
fn logic_explosion_air_only() {
    use crate::entities::comp::Health;

    let mut sim = campaign_sim();
    // Crux units (team 2) at tile (20,20): one grounded, one flying.
    let ground = sim
        .spawn_live_unit("dagger", 2, 20.0 * 8.0, 20.0 * 8.0, 0.0)
        .expect("dagger");
    let flyer = sim
        .spawn_live_unit("flare", 2, 21.0 * 8.0, 20.0 * 8.0, 0.0)
        .expect("flare");
    let health = |sim: &Sim, e| sim.ecs.0.get::<Health>(e).map_or(0.0, |h| h.health);
    let (ground_hp, flyer_hp) = (health(&sim, ground), health(&sim, flyer));
    // Sharded (team 1) explosion, air only: `explosion team x y radius damage
    // air ground pierce effect` (statement.rs `Explosion`).
    place_world_processor(
        &mut sim,
        4,
        4,
        "explosion 1 20 20 5 500 true false false false\nstop\n",
    );
    tick_n(&mut sim, 10);
    assert!(
        health(&sim, flyer) < flyer_hp,
        "the flyer takes an air-only explosion"
    );
    assert_eq!(
        health(&sim, ground),
        ground_hp,
        "the grounded unit is spared by an air-only explosion"
    );
    assert_events_drained(&sim);
}
