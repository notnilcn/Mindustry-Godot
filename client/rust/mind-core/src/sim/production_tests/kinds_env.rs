// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime block-kind, drill-boost and environment-rule regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;
use crate::sim::test_support::{
    install_campaign_session, item, place_team1, runtime_sim, stored_item, tick_n,
};

/// H02 T3: `kind_rotates` matches the Java `rotate = true` set
/// (`grep -rn "rotate = true\|rotate = false" J/world/blocks`).
#[test]
fn kind_rotates_matches_java() {
    use crate::content::BlockKind as K;
    use crate::world::block::kind_rotates;

    for kind in [
        K::Conveyor,
        K::Duct,
        K::Conduit,
        K::ItemTurret,
        K::UnitFactory,
        K::Reconstructor,
        K::HeatProducer,
        K::BeamDrill,
        K::PowerDiode,
        K::PayloadSource,
        K::ArmoredConveyor,
        K::ArmoredConduit,
        K::DuctBridge,
        K::StackRouter,
        K::Constructor,
        K::PayloadRouter,
        K::PayloadUnloader,
        K::RepairTurret,
        K::UnitAssemblerModule,
        K::WallCrafter,
    ] {
        assert!(kind_rotates(kind), "{kind:?} rotates");
    }
    for kind in [
        K::Junction,
        K::ItemBridge,
        K::Sorter,
        K::Router,
        K::OverflowGate,
        K::MassDriver,
        K::LiquidRouter,
        K::LiquidJunction,
        K::Drill,
        K::BurstDrill,
        K::Door,
        K::AutoDoor,
        K::PowerNode,
        K::BeamNode,
        K::PayloadDeconstructor,
        K::LiquidBridge,
        K::BufferedItemBridge,
    ] {
        assert!(!kind_rotates(kind), "{kind:?} does not rotate");
    }
}

/// H02 T8: per-block drill boost/warmup knobs (`Drill.java:38,40`,
/// `Blocks.java:2929,2933,3103`).
#[test]
fn blast_drill_boost_knobs_match_java() {
    use crate::world::BlockKindData;

    let sim = runtime_sim();
    let table = sim
        .ecs
        .0
        .get_resource::<crate::world::BlockTable>()
        .expect("table");
    let drill_def = |name: &str| match &table.get_named(name).expect(name).kind_data {
        BlockKindData::Drill(def) => def.clone(),
        BlockKindData::BurstDrill(def) => def.drill.clone(),
        _ => panic!("not a drill"),
    };
    let blast = drill_def("blast-drill");
    assert_eq!(blast.liquid_boost_intensity, 1.8);
    assert_eq!(blast.warmup_speed, 0.01);
    let impact = drill_def("impact-drill");
    assert_eq!(impact.liquid_boost_intensity, 1.75);
    // `Drill.java:38,40` class defaults survive everywhere else.
    let mechanical = drill_def("mechanical-drill");
    assert_eq!(mechanical.liquid_boost_intensity, 1.6);
    assert_eq!(mechanical.warmup_speed, 0.015);
}

/// H02 T8: `Mathf.approachDelta(warmup, speed, warmupSpeed)` scales with the
/// building's delta (`Drill.java:304`): one tick at time scale 2 advances the
/// warmup by two steps.
#[test]
fn drill_warmup_scales_with_time_scale() {
    let mut sim = runtime_sim();
    for x in 10..12 {
        for y in 10..12 {
            set_floor(&mut sim, x, y, "ore-copper");
        }
    }
    let drill = place_team1(&mut sim, "mechanical-drill", 10, 10);
    sim.tick().expect("tick");
    let warmup = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::entities::comp::DrillState>(drill)
            .map(|state| state.warmup)
            .unwrap_or(0.0)
    };
    assert!(
        (warmup(&sim) - 0.015).abs() < 1e-4,
        "one tick at 1x: {}",
        warmup(&sim)
    );
    {
        let mut building = sim
            .ecs
            .0
            .get_mut::<crate::entities::comp::Building>(drill)
            .expect("building");
        building.time_scale = 2.0;
        building.time_scale_duration = 1000.0;
    }
    sim.tick().expect("tick");
    assert!(
        (warmup(&sim) - 0.045).abs() < 1e-4,
        "overdrive doubles the warmup step: {}",
        warmup(&sim)
    );
}

/// H09 T4: a derelict building stops updating (`BuildingComp.allowUpdate`,
/// `BuildingComp.java:627-632`): a team-0 drill mines nothing.
#[test]
fn derelict_drill_does_not_mine() {
    let mut sim = runtime_sim();
    for (x, y) in [(5, 5), (6, 5), (5, 6), (6, 6)] {
        set_floor(&mut sim, x, y, "ore-copper");
    }
    let drill = place_team1(&mut sim, "mechanical-drill", 5, 5);
    let copper = item(&sim, "copper");
    let container = place_team1(&mut sim, "container", 7, 5);
    assert!(
        sim.debug_set_building_team(5, 5, 0),
        "team switch to derelict"
    );
    let enabled = sim
        .ecs
        .0
        .get::<crate::entities::comp::Building>(drill)
        .map(|building| building.enabled);
    assert_eq!(enabled, Some(false), "derelict drill is disabled");
    tick_n(&mut sim, 1625);
    assert_eq!(
        stored_item(&sim, container, copper),
        0,
        "a derelict drill never mines"
    );

    // Control: the same drill on team 1 mines (G02 T6 rate).
    let mut sim = runtime_sim();
    for (x, y) in [(5, 5), (6, 5), (5, 6), (6, 6)] {
        set_floor(&mut sim, x, y, "ore-copper");
    }
    place_team1(&mut sim, "mechanical-drill", 5, 5);
    let container = place_team1(&mut sim, "container", 7, 5);
    tick_n(&mut sim, 1625);
    assert!(
        stored_item(&sim, container, copper) >= 9,
        "a team-1 drill mines ~10 copper in 1625 ticks"
    );
}

/// H09 T4: buildings outside a limited map area are disabled; growing the area
/// re-enables the auto-reset ones (`World.checkMapArea`,
/// `World.java:469-481`).
#[test]
fn outside_map_area_disabled_and_reset() {
    use crate::entities::comp::Building;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;

    let mut sim = runtime_sim();
    let drill = place_team1(&mut sim, "mechanical-drill", 20, 20);
    let mut rules = Rules {
        default_team: 1,
        ..Rules::default()
    };
    rules.limit_map_area = true;
    rules.limit_width = 16;
    rules.limit_height = 16;
    rules.disable_outside_area = true;
    install_campaign_session(&mut sim, rules.clone());
    sim.apply_rules_to_world(&rules);
    assert_eq!(
        sim.ecs.0.get::<Building>(drill).map(|b| b.enabled),
        Some(false),
        "a drill outside the 16x16 area is disabled"
    );

    rules.limit_width = 32;
    rules.limit_height = 32;
    if let Some(mut runtime) = sim.ecs.0.get_resource_mut::<CampaignRuntime>() {
        runtime.session.rules = rules.clone();
    }
    sim.apply_rules_to_world(&rules);
    assert_eq!(
        sim.ecs.0.get::<Building>(drill).map(|b| b.enabled),
        Some(true),
        "growing the area re-enables the auto-reset drill"
    );
}

/// H09 T4: a block whose env flags do not support the planet's env stops
/// updating (`Block.supportsEnv`, `BuildingComp.allowUpdate`).
#[test]
fn env_unsupported_block_disabled() {
    use crate::entities::comp::Building;
    use crate::game::rules::Rules;
    use crate::world::build::supports_env_bits;

    let mut sim = runtime_sim();
    let name = "cultivator";
    let def = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_by_name(name)
        .expect("def")
        .clone();
    // Erekir env (17 = space | scorching).
    assert!(
        !supports_env_bits(&def, 17),
        "{name} (requires spores) is not supported on Erekir"
    );
    let generator = place_team1(&mut sim, name, 5, 5);
    let mut rules = Rules {
        default_team: 1,
        ..Rules::default()
    };
    rules.env = 17;
    install_campaign_session(&mut sim, rules.clone());
    sim.apply_rules_to_world(&rules);
    assert_eq!(
        sim.ecs.0.get::<Building>(generator).map(|b| b.enabled),
        Some(false),
        "{name} is disabled under env 17"
    );
}
