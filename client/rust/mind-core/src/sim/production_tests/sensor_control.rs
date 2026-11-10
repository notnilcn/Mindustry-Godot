// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime world-processor sensor and control regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;
use crate::sim::test_support::{configure, item, place_team1, runtime_sim, tick_n};

// ---- G06 T7 (sensor/control coverage) ----

/// Ground Zero's `duoAmmo` objective: `sensor amount d @ammo` on a duo reads
/// `TurretState.total_ammo` (`ItemTurretBuild.sense`), not 0.
#[test]
fn duo_ammo_sensed() {
    let mut sim = runtime_sim();
    let duo = place_team1(&mut sim, "duo", 20, 20);
    let copper = item(&sim, "copper");
    let world = &mut sim.ecs.0;
    assert!(transfer::dispatch_accept_item(world, duo, duo, copper));
    for _ in 0..5 {
        transfer::dispatch_handle_item(world, duo, duo, copper);
    }
    let total = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .map(|state| state.total_ammo)
        .unwrap_or(0);
    assert!(total > 0, "turret holds ammo");
    use crate::logic::LAccess;
    use crate::logic::access::{Sensed, sense};
    use crate::logic::value::LogicObject;
    let target = LogicObject::Building(duo);
    assert_eq!(
        sense(&sim.ecs.0, &target, LAccess::Ammo),
        Sensed::Num(total as f64)
    );
    let cap = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .map(|state| state.config.max_ammo)
        .unwrap_or(0);
    assert_eq!(
        sense(&sim.ecs.0, &target, LAccess::AmmoCapacity),
        Sensed::Num(cap as f64)
    );
    assert_eq!(
        sense(&sim.ecs.0, &target, LAccess::Shooting),
        Sensed::Num(0.0)
    );
}

/// `@x/@y` match Java's building centre: 1x1/3x3 -> tile, 2x2 -> tile + 0.5.
#[test]
fn building_xy_tiles() {
    let mut sim = runtime_sim();
    let hyper = place_team1(&mut sim, "hyper-processor", 10, 10);
    let logic = place_team1(&mut sim, "logic-processor", 20, 20);
    let micro = place_team1(&mut sim, "micro-processor", 3, 4);
    use crate::logic::LAccess;
    use crate::logic::access::{Sensed, sense};
    use crate::logic::value::LogicObject;
    let x = |e| sense(&sim.ecs.0, &LogicObject::Building(e), LAccess::X);
    let y = |e| sense(&sim.ecs.0, &LogicObject::Building(e), LAccess::Y);
    assert_eq!(x(hyper), Sensed::Num(10.0));
    assert_eq!(x(logic), Sensed::Num(20.5));
    assert_eq!(x(micro), Sensed::Num(3.0));
    assert_eq!(y(micro), Sensed::Num(4.0));
}

/// G06 T3 (PAR-0107): `lookup item r 0` resolves through `logicids.dat`.
#[test]
fn lookup_resolves_content_from_logic_ids() {
    let mut sim = runtime_sim();
    let (globals, copper) = {
        let content = sim.block_runtime().expect("runtime").content();
        (
            crate::logic::globals::GlobalVars::with_content(content),
            content.item_id("copper").expect("copper"),
        )
    };
    sim.ecs.0.insert_resource(globals);
    let id = sim.world_block_id("micro-processor").expect("block");
    sim.command(SimCommand::Place {
        x: 10,
        y: 10,
        block: id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("placed");
    let processor = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
    let bytes = crate::logic::blocks::logic_block::compress(
        "lookup item r 0\nlookup team t 1\nstop\n",
        &[],
    );
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    tick_n(&mut sim, 4);
    let state = sim
        .ecs
        .0
        .get::<crate::logic::blocks::LogicBlockState>(processor)
        .expect("state");
    let r = state.executor.optional_var("r").expect("r");
    assert!(
        matches!(
            &state.executor.arena.get(r).obj,
            Some(crate::logic::value::LogicObject::Content(c))
                if c.type_ == crate::content::ContentType::Item && c.id == copper.raw()
        ),
        "lookup item 0 -> copper"
    );
    let t = state.executor.optional_var("t").expect("t");
    assert_eq!(state.executor.arena.get(t).num(), 1.0, "team 1 = sharded");
}
