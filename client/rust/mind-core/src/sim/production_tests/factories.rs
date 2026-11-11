// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime reconstructor, message-block and unit-factory regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;
use crate::sim::test_support::{item, place_team1, runtime_sim, stored_item, tick_n};

/// PAR-0076: an additive reconstructor only upgrades a payload with power and
/// its 40 silicon + 40 graphite, after `constructTime` (600) ticks, and consumes
/// those items on completion.
#[test]
fn reconstructor_needs_items_and_time() {
    let mut sim = runtime_sim();
    let _source = place_team1(&mut sim, "power-source", 3, 5);
    let rig = place_team1(&mut sim, "additive-reconstructor", 5, 5);
    let (silicon, graphite) = (item(&sim, "silicon"), item(&sim, "graphite"));
    let dagger = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    let mace = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("mace")
        .expect("mace");

    // Load a dagger payload through the behavior hook (`acceptPayload`).
    let inst = sim
        .ecs
        .0
        .resource::<crate::world::block::BlockTable>()
        .instance(
            sim.ecs
                .0
                .get::<crate::entities::comp::Building>(rig)
                .expect("building")
                .block,
        )
        .expect("instance");
    inst.behavior.handle_payload(
        &mut sim.ecs.0,
        rig,
        rig,
        crate::world::behavior::PayloadRef {
            entity: None,
            content: dagger.raw(),
            is_block: false,
        },
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::world::blocks::units::ReconstructorBuild>(rig)
            .and_then(|state| state.payload),
        Some(dagger)
    );

    // No items: the payload waits, nothing spawns.
    tick_n(&mut sim, 700);
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::world::blocks::units::ReconstructorBuild>(rig)
            .and_then(|state| state.payload),
        Some(dagger),
        "payload waits without items"
    );
    assert!(!spawned_unit(&sim, mace), "no upgrade without items");

    // Feed the required stacks: the upgrade lands after constructTime ticks.
    {
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(rig) {
            items.add(silicon, 40, 80);
            items.add(graphite, 40, 80);
        }
    }
    let start = sim.tick_count();
    for _ in 0..900 {
        sim.tick().expect("tick");
        if spawned_unit(&sim, mace) {
            break;
        }
    }
    let elapsed = sim.tick_count() - start;
    assert!(spawned_unit(&sim, mace), "upgraded to a mace");
    assert!(
        (600..=700).contains(&elapsed),
        "construct time {elapsed} ticks"
    );
    assert_eq!(stored_item(&sim, rig, silicon), 0, "silicon consumed");
    assert_eq!(stored_item(&sim, rig, graphite), 0, "graphite consumed");
}

/// §7.0 (config prerequisite): free text reaches a building through the wire
/// `Text` value and reads back as the behavior's `String` config.
#[test]
fn message_block_accepts_text_config() {
    let mut sim = runtime_sim();
    let message = place_team1(&mut sim, "message", 5, 5);
    sim.command(SimCommand::Configure {
        x: 5,
        y: 5,
        value: crate::determinism::ConfigValue::Text("hello\nworld".to_owned()),
    })
    .expect("configure");
    assert_eq!(
        crate::world::config::read_config(&sim.ecs.0, message),
        crate::world::config::ConfigValue::String("hello\nworld".to_owned())
    );
}

/// PAR-0092 / J05 T2: `UnitFactory.config(UnitType.class)` selects the matching
/// plan (resetting progress) and `config()` reports the plan's unit
/// (`UnitFactory.java:375-378`, `build.currentPlan == -1 ? null :
/// plans.get(currentPlan).unit`). `config(Integer.class)` with `-1` clears the
/// plan; `configure(null)` (`configClear`, `:82`) only clears the *command*.
/// With no plan selected nothing is produced even with the items in storage.
#[test]
fn unit_factory_config_selects_plan() {
    let mut sim = runtime_sim();
    let _source = place_team1(&mut sim, "power-source", 3, 5);
    let factory = place_team1(&mut sim, "air-factory", 5, 5);
    let silicon = item(&sim, "silicon");
    let content_flare = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("flare")
        .expect("flare");
    let mono = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id("mono")
        .expect("mono");

    // `created()` auto-selects the first `unlockedNow()` plan; the runtime
    // registry never carries campaign research, so the first plan (flare) is
    // the fallback default. `config()` reports that plan's unit type.
    assert_eq!(
        crate::world::config::read_config(&sim.ecs.0, factory),
        crate::world::config::ConfigValue::Unit(content_flare)
    );

    // Configure a plan by unit name (the config panel's content submit path).
    sim.command(SimCommand::Configure {
        x: 5,
        y: 5,
        value: crate::determinism::ConfigValue::Content("mono".to_owned()),
    })
    .expect("configure mono");
    assert_eq!(
        crate::world::config::read_config(&sim.ecs.0, factory),
        crate::world::config::ConfigValue::Unit(mono)
    );

    // `configure(null)` / `configClear` (`UnitFactory.java:82`) clears only the
    // command; the plan stays selected. `config(Integer.class)` with `-1`
    // (`:58-67`) is the way to unset the plan.
    sim.command(SimCommand::Configure {
        x: 5,
        y: 5,
        value: crate::determinism::ConfigValue::None,
    })
    .expect("clear command");
    assert_eq!(
        crate::world::config::read_config(&sim.ecs.0, factory),
        crate::world::config::ConfigValue::Unit(mono),
        "configClear keeps the plan"
    );
    sim.command(SimCommand::Configure {
        x: 5,
        y: 5,
        value: crate::determinism::ConfigValue::Int(-1),
    })
    .expect("clear plan");
    assert_eq!(
        crate::world::config::read_config(&sim.ecs.0, factory),
        crate::world::config::ConfigValue::None
    );
    {
        let world = &mut sim.ecs.0;
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(factory) {
            items.add(silicon, 60, 100);
        }
    }
    tick_n(&mut sim, 120);
    assert!(
        !spawned_unit(&sim, content_flare) && !spawned_unit(&sim, mono),
        "no plan selected -> nothing produced"
    );

    // Re-select flare: the 15 silicon plan completes after its 900 ticks.
    sim.command(SimCommand::Configure {
        x: 5,
        y: 5,
        value: crate::determinism::ConfigValue::Content("flare".to_owned()),
    })
    .expect("configure flare");
    for _ in 0..1000 {
        sim.tick().expect("tick");
        if spawned_unit(&sim, content_flare) {
            break;
        }
    }
    assert!(spawned_unit(&sim, content_flare), "flare produced");
}
