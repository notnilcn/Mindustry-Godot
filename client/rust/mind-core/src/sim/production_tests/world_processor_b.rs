// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live world-processor regressions, second half: cutscenes, messages, setprop and sector programs.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;
use crate::sim::test_support::{
    campaign_runtime, campaign_runtime_mut, configure, install_campaign_session,
    install_logic_world, item, place_team1, place_world_processor, runtime_sim, stored_item,
    test_rules, tick_n,
};

/// H06 T3: Ground Zero GZ-4 verbatim — the pan activates, the map area is
/// expanded and the `wait 11` + `cutscene stop` end the cutscene.
#[test]
fn gz4_cutscene_runs_and_stops() {
    let mut sim = Sim::new(3, 300, 300, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    install_campaign_session(&mut sim, test_rules());
    install_logic_world(&mut sim);
    {
        let mut runtime = campaign_runtime_mut(&mut sim);
        runtime
            .session
            .rules
            .objective_flags
            .insert("mineOver".to_owned());
        runtime
            .session
            .rules
            .objective_flags
            .insert("spawnOver".to_owned());
    }
    place_world_processor(
        &mut sim,
        10,
        10,
        "getflag mineOver \"mineOver\"\njump 0 equal mineOver false\nsetflag \"mineOver\" false\nsetrule mapArea 10 0 0 256 256\ngetflag end \"spawnOver\"\njump 4 equal end false\ncutscene pan 135 230 0.03 0\nwait 11\ncutscene stop 100 100 0.06 0\nstop\n",
    );
    tick_n(&mut sim, 5);
    let cutscene = sim.logic_cutscene();
    assert!(cutscene.active, "GZ-4 pan active after the wait starts");
    assert_eq!(cutscene.pan, (1080.0, 1840.0));
    {
        let runtime = campaign_runtime(&sim);
        assert!(runtime.session.rules.limit_map_area, "map area set");
        assert_eq!(runtime.session.rules.limit_width, 256);
        assert_eq!(runtime.session.rules.limit_height, 256);
    }
    // `wait 11` (seconds) then `cutscene stop`.
    tick_n(&mut sim, 11 * 60 + 10);
    assert!(!sim.logic_cutscene().active, "stop ends the cutscene");
}

/// H06 T3: headless (`CutsceneI.run`: `if(headless) return;`) leaves no
/// presentation state.
#[test]
fn cutscene_headless_noop() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    sim.ecs
        .0
        .insert_resource(crate::logic::world::LogicWorldState::new());
    place_world_processor(
        &mut sim,
        10,
        10,
        "cutscene pan 10 20 0.05 0\ncutscene active a 0 0 0\nstop\n",
    );
    tick_n(&mut sim, 4);
    assert_eq!(
        sim.logic_cutscene(),
        crate::logic::world::LogicCutscene::default()
    );
}

/// H06 T5: an unchanged `setrule` does not request a world re-derive
/// (frozenForest/fungalPass re-set `waveTimer` every tick).
#[test]
fn setrule_unchanged_is_not_dirty() {
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    let mut rules = test_rules();
    rules.wave_timer = true;
    rules.waves = true;
    install_campaign_session(&mut sim, rules);
    // Sabotage the derived world copy: if the setrule re-derived the world,
    // `sync_build_rules` would copy `rules.waves = true` back.
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("build rules")
        .waves = false;
    place_world_processor(
        &mut sim,
        10,
        10,
        "setrule waveTimer true 0 0 0 0\nsetrule waves true 0 0 0 0\nstop\n",
    );
    tick_n(&mut sim, 4);
    let runtime = campaign_runtime(&sim);
    assert!(runtime.session.rules.wave_timer);
    assert!(runtime.session.rules.waves);
    assert!(
        !runtime.rules_world_dirty,
        "no change -> no re-derive request"
    );
    assert!(
        !sim.ecs
            .0
            .get_resource::<BuildRules>()
            .expect("build rules")
            .waves,
        "the world copy was not re-synced"
    );
}

/// H06 T5: a real change marks the session dirty so the world re-derive runs
/// and `sync_build_rules` reaches `BuildRules`.
#[test]
fn setrule_waves_resyncs_build_rules() {
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules()); // waves = false
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("build rules")
        .waves = false;
    place_world_processor(&mut sim, 10, 10, "setrule waves true 0 0 0 0\nstop\n");
    tick_n(&mut sim, 4);
    let runtime = campaign_runtime(&sim);
    assert!(runtime.session.rules.waves);
    assert!(
        sim.ecs
            .0
            .get_resource::<BuildRules>()
            .expect("build rules")
            .waves,
        "the changed rule drove the world re-sync"
    );
}

/// H06 T5: per-team `setrule` marks the session dirty (the world re-derive
/// runs; `grid.border_darkness` is one of its writes).
#[test]
fn per_team_rule_changes_mark_dirty() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    // `@sharded` resolves through the content globals.
    let content = crate::world::harness::BuildHarness::load_content();
    crate::logic::globals::GlobalVars::install_world(&mut sim.ecs.0, &content);
    // Out-of-band world copy of a re-derived field.
    sim.grid.border_darkness = false;
    place_world_processor(
        &mut sim,
        10,
        10,
        "setrule buildSpeed 2 @sharded 0 0 0\nstop\n",
    );
    tick_n(&mut sim, 4);
    let runtime = campaign_runtime(&sim);
    assert_eq!(
        runtime.session.rules.team_rule(1).build_speed_multiplier,
        2.0
    );
    assert!(
        sim.grid.border_darkness,
        "the world re-derive ran (dirty was set on the per-team change)"
    );
}

/// H06 T5: `mapArea` re-enables a building disabled outside the previous area
/// (Java `World.checkMapArea`, World.java:469-481).
#[test]
fn map_area_reenables_outside_buildings() {
    use crate::entities::comp::Building;

    let mut sim = Sim::new(3, 300, 300, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    let mut rules = test_rules();
    rules.limit_map_area = true;
    rules.limit_x = 0;
    rules.limit_y = 0;
    rules.limit_width = 20;
    rules.limit_height = 20;
    rules.disable_outside_area = true;
    install_campaign_session(&mut sim, rules.clone());
    // Place before the limit is applied to the grid (placement outside the
    // rect is refused once `grid.limit_area` is set).
    let conveyor = place_team1(&mut sim, "conveyor", 25, 25);
    sim.apply_rules_to_world(&rules);
    assert!(
        !sim.ecs
            .0
            .get::<Building>(conveyor)
            .expect("building")
            .enabled,
        "disabled outside the 20x20 area"
    );
    place_world_processor(&mut sim, 10, 10, "setrule mapArea 10 0 0 50 50\nstop\n");
    tick_n(&mut sim, 3);
    assert!(
        sim.ecs
            .0
            .get::<Building>(conveyor)
            .expect("building")
            .enabled,
        "re-enabled by checkMapArea(prev)"
    );
}

/// `message announce 2 @wait` blocks until the announcement timer expires.
#[test]
fn message_wait_blocks_until_announcement_expires() {
    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    sim.ecs
        .0
        .insert_resource(crate::logic::world::LogicWorldState {
            headless: false,
            ..crate::logic::world::LogicWorldState::new()
        });
    sim.ecs
        .0
        .insert_resource(crate::logic::world::MessageState::default());
    place_world_processor(
        &mut sim,
        10,
        10,
        "message announce 2 0\nmessage announce 2 @wait\nset x 2\nstop\n",
    );
    let state_at = |sim: &Sim| {
        let processor = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
        sim.ecs
            .0
            .get::<crate::logic::blocks::LogicBlockState>(processor)
            .expect("state")
            .executor
            .optional_var("x")
            .map(|id| {
                sim.ecs
                    .0
                    .get::<crate::logic::blocks::LogicBlockState>(processor)
                    .expect("state")
                    .executor
                    .arena
                    .get(id)
                    .num()
            })
            .unwrap_or(0.0)
    };
    tick_n(&mut sim, 10);
    {
        let messages = sim
            .ecs
            .0
            .get_resource::<crate::logic::world::MessageState>()
            .expect("messages");
        let tick = sim
            .ecs
            .0
            .get_resource::<crate::logic::world::LogicWorldState>()
            .expect("state")
            .tick;
        assert!(
            messages.announce_until > 0,
            "first announce armed the timer (tick={tick})"
        );
        assert!(
            tick < messages.announce_until,
            "still inside the announcement window (tick={tick}, until={})",
            messages.announce_until
        );
    }
    assert_eq!(state_at(&sim), 0.0, "second announce is still blocked");
    tick_n(&mut sim, 140);
    assert_eq!(state_at(&sim), 2.0, "wait released after the announcement");
}

/// H06 T4: `take_logic_messages` drains queued `announce`/`notify`/`toast`
/// events (resolved text + duration) exactly once.
#[test]
fn message_announce_drained() {
    use crate::logic::enums::MessageType;

    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    install_logic_world(&mut sim);
    place_world_processor(
        &mut sim,
        10,
        10,
        "print \"hi\"\nmessage announce 3 ok\nstop\n",
    );
    tick_n(&mut sim, 3);
    let messages = sim.take_logic_messages();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert_eq!(messages[0].0, MessageType::Announce);
    assert_eq!(messages[0].1, "hi");
    assert!((messages[0].2 - 3.0).abs() < 1e-6);
    assert!(
        sim.take_logic_messages().is_empty(),
        "messages drain exactly once"
    );
    // `outSuccess` stays 1 for a delivered (non-blocked) message.
    let e = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
    let ok = sim
        .ecs
        .0
        .get::<crate::logic::blocks::LogicBlockState>(e)
        .and_then(|state| {
            state
                .executor
                .optional_var("ok")
                .map(|id| state.executor.arena.get(id).num())
        });
    assert_eq!(ok, Some(1.0));
}

/// H06 T6: `setprop` on an item stack SETS the count (`BuildingComp.setProp`,
/// `BuildingComp.java:2210-2227`), not adds; re-applying is a no-op.
#[test]
fn setprop_item_sets_count() {
    use crate::content::ContentRef;
    use crate::logic::access::set_prop_content;
    use crate::logic::value::LogicObject;

    let mut sim = runtime_sim();
    let container = place_team1(&mut sim, "container", 5, 5);
    let copper = item(&sim, "copper");
    let target = LogicObject::Building(container);
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &target,
        ContentRef::item(copper),
        50.0
    ));
    assert_eq!(stored_item(&sim, container, copper), 50);
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &target,
        ContentRef::item(copper),
        10.0
    ));
    assert_eq!(stored_item(&sim, container, copper), 10, "set, not added");
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &target,
        ContentRef::item(copper),
        10.0
    ));
    assert_eq!(stored_item(&sim, container, copper), 10, "idempotent");

    // A core accepts up to its capacity (`getMaximumAccepted`), not 99999.
    let core = place_team1(&mut sim, "core-shard", 10, 10);
    let capacity = sim
        .ecs
        .0
        .get::<crate::world::blocks::storage::core_block::CoreBuild>(core)
        .map(|core| core.storage_capacity)
        .expect("core build");
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &LogicObject::Building(core),
        ContentRef::item(copper),
        99999.0
    ));
    let stored = stored_item(&sim, core, copper);
    assert!(
        stored <= capacity,
        "capacity-limited: {stored} <= {capacity}"
    );
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &LogicObject::Building(core),
        ContentRef::item(copper),
        99999.0
    ));
    assert_eq!(stored_item(&sim, core, copper), stored, "stable on repeat");
}

/// H06 T6: `setprop @health u 0` kills the unit through the combat path
/// (`UnitComp.setProp` health; `UnitComp.java:356-362`).
#[test]
fn setprop_unit_health_zero_kills() {
    use crate::logic::LAccess;
    use crate::logic::access::set_prop_num;
    use crate::logic::value::LogicObject;

    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let unit = sim
        .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
        .expect("dagger");
    assert!(set_prop_num(
        &mut sim.ecs.0,
        &LogicObject::Unit(unit),
        LAccess::Health,
        0.0
    ));
    tick_n(&mut sim, 2);
    assert!(
        sim.ecs.0.get_entity(unit).is_err(),
        "the unit was destroyed by resolve_deaths"
    );
}

/// H06 T6: `setprop` unit item stack + status duration + `@speed`/`@armor`
/// dynamic status (`UnitComp.setProp`; the multipliers survive ticks).
#[test]
fn setprop_unit_stack_and_status() {
    use crate::content::ContentRef;
    use crate::entities::comp::unit::comp::{DynamicStatus, ItemsComp, StatusComp};
    use crate::logic::LAccess;
    use crate::logic::access::{set_prop_content, set_prop_num};
    use crate::logic::value::LogicObject;

    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let unit = sim
        .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
        .expect("dagger");
    let copper = item(&sim, "copper");
    let wet = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .status_id("wet")
        .expect("wet");
    let dagger_speed = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .and_then(|id| sim.block_runtime().expect("runtime").content().unit(id))
        .map(|def| def.speed)
        .expect("dagger def");
    let target = LogicObject::Unit(unit);
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &target,
        ContentRef::item(copper),
        20.0
    ));
    assert_eq!(
        sim.ecs
            .0
            .get::<ItemsComp>(unit)
            .and_then(|items| items.item),
        Some((copper, 20)),
        "unit stack set"
    );
    assert!(set_prop_content(
        &mut sim.ecs.0,
        &target,
        ContentRef::new(crate::content::ContentType::Status, wet.raw()),
        3.0
    ));
    assert!(
        (sim.ecs
            .0
            .get::<StatusComp>(unit)
            .expect("status")
            .get_duration(wet)
            - 180.0)
            .abs()
            < 1e-3,
        "3 s = 180 ticks"
    );
    // `@speed` -> `applyDynamicStatus().speedMultiplier = v / (speed * 60 / 8)`.
    assert!(set_prop_num(&mut sim.ecs.0, &target, LAccess::Speed, 10.0));
    let dynamic = sim
        .ecs
        .0
        .get::<DynamicStatus>(unit)
        .copied()
        .expect("dynamic status");
    assert!(
        (dynamic.speed_multiplier - 10.0 / (dagger_speed * 60.0 / 8.0)).abs() < 1e-4,
        "{:?}",
        dynamic.speed_multiplier
    );
    assert!(set_prop_num(&mut sim.ecs.0, &target, LAccess::Armor, 5.0));
    // A few ticks: the per-tick status update folds the dynamic multipliers in
    // instead of resetting the override (the old bug).
    tick_n(&mut sim, 3);
    let status = sim.ecs.0.get::<StatusComp>(unit).expect("status").clone();
    assert!(
        (status.armor_override - 5.0).abs() < 1e-4,
        "armor override survives ticks: {}",
        status.armor_override
    );
    assert!(
        status.has_effect_of(
            sim.block_runtime()
                .expect("runtime")
                .content()
                .status_id("dynamic")
                .expect("dynamic id")
        )
    );
}

/// H06 T6: extractionOutpost block 2 verbatim on a team-2 core linked as
/// `nucleus1` -> `setprop @pyratite 99999` fills to the capacity limit.
#[test]
fn extraction_outpost_block2() {
    use crate::logic::blocks::{LogicLink, logic_block};
    use crate::logic::value::LogicObject;

    let mut sim = runtime_sim();
    install_campaign_session(&mut sim, test_rules());
    let content = crate::world::harness::BuildHarness::load_content();
    crate::logic::globals::GlobalVars::install_world(&mut sim.ecs.0, &content);
    // Team-2 core-nucleus (its link name root is `nucleus`).
    let core_id = sim.world_block_id("core-nucleus").expect("core-nucleus");
    sim.command(SimCommand::Place {
        x: 14,
        y: 10,
        block: core_id,
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("core placed");
    let core = sim
        .grid
        .entity_at(TilePos::new(14, 10))
        .expect("core entity");
    // World processor at (10,10) with the core linked as a relative `(4, 0)`.
    let code = "sensor result nucleus1 @team\njump 0 notEqual result 2\nsetprop @pyratite nucleus1 99999\nsetprop @metaglass nucleus1 99999\nsetprop @titanium nucleus1 99999\nsetprop @graphite nucleus1 99999\n";
    let processor = sim
        .world_block_id("world-processor")
        .expect("world-processor");
    sim.command(SimCommand::Place {
        x: 10,
        y: 10,
        block: processor,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("processor placed");
    let bytes = logic_block::compress(code, &[LogicLink::new("nucleus1", 4, 0)]);
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    tick_n(&mut sim, 4);
    let entity = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
    let state = sim
        .ecs
        .0
        .get::<crate::logic::blocks::LogicBlockState>(entity)
        .expect("state");
    let d = state.executor.optional_var("nucleus1");
    assert!(d.is_some(), "the link resolved to a variable");
    assert!(
        state
            .executor
            .arena
            .get(d.expect("id"))
            .value_obj()
            .is_some_and(|obj| matches!(obj, LogicObject::Building(e) if *e == core)),
        "nucleus1 binds the core"
    );
    let pyratite = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .item_id("pyratite")
        .expect("pyratite");
    let stored = stored_item(&sim, core, pyratite);
    let capacity = sim
        .ecs
        .0
        .get::<crate::world::blocks::storage::core_block::CoreBuild>(core)
        .map(|core| core.storage_capacity)
        .expect("core build");
    assert!(stored > 0, "pyratite set on the core");
    assert_eq!(stored, 99999.min(capacity), "capacity-limited set");
}
