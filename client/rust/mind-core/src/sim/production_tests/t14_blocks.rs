// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime separator, mass-driver, generator and bridge regressions (T14 blocks).
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;

// T14 tests (to append to client/rust/mind-core/src/sim/production_tests.rs)

/// H02 T14: `Separator` weighted outputs (`Blocks.java:1233-1249`:
/// `results = with(copper 5, lead 3, graphite 2, titanium 2)`,
/// `craftTime = 35`; pick is a uniform draw over the weight sum,
/// `Separator.java:132-162`).
#[test]
fn separator_weighted_results() {
    let mut sim = runtime_sim();
    let separator = place(&mut sim, "separator", 5, 5);
    let _power = place(&mut sim, "power-source", 7, 5);
    let _slag = place(&mut sim, "liquid-source", 4, 5);
    configure(
        &mut sim,
        4,
        5,
        crate::determinism::ConfigValue::Content("slag".to_owned()),
    );
    let results = [
        (item(&sim, "copper"), 41.67f32),
        (item(&sim, "lead"), 25.0),
        (item(&sim, "graphite"), 16.67),
        (item(&sim, "titanium"), 16.67),
    ];
    let mut counts = [0i32; 4];
    // 600 crafts at craftTime 35. The outputs are drained every tick so the
    // separator's itemCapacity never stalls the craft loop.
    for _ in 0..600 * 35 {
        sim.tick().expect("tick");
        if let Some(mut module) = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(separator)
        {
            for (slot, (id, _)) in results.iter().enumerate() {
                let held = module.get(*id);
                if held > 0 {
                    counts[slot] += held;
                    module.remove(*id, held);
                }
            }
        }
    }
    let total: i32 = counts.iter().sum();
    assert!(
        total >= 550,
        "separator crafted {total} items in 21000 ticks"
    );
    for (slot, (_, expected)) in results.iter().enumerate() {
        let pct = 100.0 * counts[slot] as f32 / total as f32;
        assert!(
            (pct - expected).abs() <= 5.0,
            "result {slot} {pct}% vs Java {expected}% (counts {counts:?})"
        );
    }
}

/// H02 T14: a configured mass driver shoots its payload to the linked driver
/// (`MassDriver.java` `updateTile`/`shoot`; `Blocks.java` `mass-driver`).
#[test]
fn mass_driver_shoots_to_linked() {
    let mut sim = runtime_sim();
    let first = place(&mut sim, "mass-driver", 5, 5);
    let second = place(&mut sim, "mass-driver", 25, 5);
    let _power_a = place(&mut sim, "power-source", 3, 5);
    let _power_b = place(&mut sim, "power-source", 27, 5);
    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::Int(TilePos::new(25, 5).pack()),
    );
    let copper = item(&sim, "copper");
    if let Some(mut module) = sim
        .ecs
        .0
        .get_mut::<crate::world::modules::ItemModule>(first)
    {
        module.add(copper, 50, 200);
    }
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let arrived = stored_item(&sim, second, copper);
    assert!(
        arrived > 0,
        "the linked mass driver delivered {arrived} copper in 600 ticks"
    );
}

/// H02 T14: `ThermalGeneratorBuild.updateTile` (`ThermalGenerator.java:87-89`):
/// `productionEfficiency = sumAttribute(heat) + Attribute.heat.env()`, power =
/// `powerProduction * productionEfficiency`. `Blocks.java:2543-2552`:
/// `powerProduction = 1.8`; `hotrock` heat is 0.5 (`Blocks.java:364-372`).
#[test]
fn thermal_generator_reads_heat_floor() {
    let mut sim = runtime_sim();
    for (x, y) in [(5, 5), (6, 5), (5, 6), (6, 6)] {
        set_floor(&mut sim, x, y, "hotrock");
    }
    let generator = place(&mut sim, "thermal-generator", 5, 5);
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
    let expected = 1.8 * 0.5 * 4.0;
    let production = sim
        .ecs
        .0
        .get::<crate::world::blocks::power::PowerProduction>(generator)
        .map(|p| p.0)
        .unwrap_or(0.0);
    let sum = sim
        .ecs
        .0
        .get::<crate::world::blocks::power::behavior::ThermalState>(generator)
        .map(|state| state.sum)
        .unwrap_or(-1.0);
    let efficiency = sim
        .ecs
        .0
        .get::<crate::world::blocks::power::generator::GeneratorState>(generator)
        .map(|state| state.production_efficiency)
        .unwrap_or(-1.0);
    assert!(
        (production - expected).abs() < 0.001,
        "thermal production {production} vs powerProduction * sum(heat) = {expected} (sum {sum}, efficiency {efficiency})"
    );
}

/// H02 T14: `steam-generator` needs its water consumer (`Blocks.java:2554-2560`
/// `consumeLiquid(Liquids.water, 0.1f)`): coal alone produces nothing.
#[test]
fn steam_generator_needs_water() {
    let mut sim = runtime_sim();
    let generator = place(&mut sim, "steam-generator", 5, 5);
    let coal = item(&sim, "coal");
    let water = liquid(&sim, "water");
    if let Some(mut module) = sim
        .ecs
        .0
        .get_mut::<crate::world::modules::ItemModule>(generator)
    {
        module.add(coal, 20, 200);
    }
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let dry = sim
        .ecs
        .0
        .get::<crate::world::blocks::power::PowerProduction>(generator)
        .map(|p| p.0)
        .unwrap_or(0.0);
    assert_eq!(dry, 0.0, "no water -> no production");
    if let Some(mut module) = sim
        .ecs
        .0
        .get_mut::<crate::world::modules::LiquidModule>(generator)
    {
        module.add(water, 100.0, 1000.0);
    }
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let wet = sim
        .ecs
        .0
        .get::<crate::world::blocks::power::PowerProduction>(generator)
        .map(|p| p.0)
        .unwrap_or(0.0);
    assert!(wet > 0.0, "water -> production {wet}");
}

/// H02 T14: an unloader pulls a graphite press's output out (G02's generic
/// crafter test, pinned to the early-game chain).
#[test]
fn unloader_pulls_from_graphite_press() {
    let mut sim = runtime_sim();
    // Receiver lane: container <- belt(9,5) <- belt(8,5) <- unloader. Belts
    // resolve their forward target when placed, so build downstream-first.
    let container = place(&mut sim, "container", 10, 5);
    let _b2 = place(&mut sim, "conveyor", 9, 5);
    let _b1 = place(&mut sim, "conveyor", 8, 5);
    let press = place(&mut sim, "graphite-press", 5, 5);
    let _unloader = place(&mut sim, "unloader", 7, 5);
    let coal = item(&sim, "coal");
    let graphite = item(&sim, "graphite");
    if let Some(mut module) = sim
        .ecs
        .0
        .get_mut::<crate::world::modules::ItemModule>(press)
    {
        module.add(coal, 20, 200);
    }
    for _ in 0..1000 {
        sim.tick().expect("tick");
    }
    assert!(
        stored_item(&sim, container, graphite) > 0,
        "graphite left the press (press {}, container {})",
        stored_item(&sim, press, graphite),
        stored_item(&sim, container, graphite)
    );
}

/// H02 T14: buffered bridge throughput. `BufferedItemBridge.updateTransport`
/// (`BufferedItemBridge.java:29-36`) sends one item per
/// `timer(timerAccept, 4)` = 15/s once the pipeline is full; `speed = 74`
/// (`Blocks.java:2111`) is the head item's ready latency, `bufferCapacity = 14`
/// (`:2113`). The bridge is fed directly: the titanium-conveyor rate
/// (4.8/s) is below the bridge's, so this pins the bridge.
#[test]
fn bridge_conveyor_throughput() {
    let mut sim = runtime_sim();
    let a = place(&mut sim, "bridge-conveyor", 5, 5);
    let b = place(&mut sim, "bridge-conveyor", 8, 5);
    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::Int(TilePos::new(8, 5).pack()),
    );
    let copper = item(&sim, "copper");
    let mut arrived = 0;
    for tick in 0..600 {
        sim.tick().expect("tick");
        let now = crate::world::update::build_time(&sim.ecs.0);
        if let Some(mut bridge) = sim
            .ecs
            .0
            .get_mut::<crate::world::blocks::distribution::ItemBridgeBuild>(a)
            && let Some(buffer) = bridge.buffer.as_mut()
        {
            // Stamped in the past: `speed = 74` is the head item's ready
            // latency, so this isolates the send timer (the sustained rate).
            buffer.accept(copper, -1, now - 200.0);
        }
        if let Some(mut module) = sim.ecs.0.get_mut::<crate::world::modules::ItemModule>(b) {
            if tick >= 200 {
                arrived += module.get(copper);
            }
            module.remove(copper, 1_000_000);
        }
    }
    // `timer(timerAccept, 4)` fires every 5 ticks (the timer is set to the
    // interval and only decremented on later calls, `BuildingComp.timer`), so
    // the sustained rate is 12 items/s: 400 ticks / 5 = 80, +-10 %.
    assert!(
        (72..=88).contains(&arrived),
        "bridge moved {arrived} items in the 400-tick window (Java: 80)"
    );
}

/// H02 T15: `BeamNode.BeamNodeBuild.updateDirections` (`BeamNode.java:241-293`)
/// — a beam node links the first same-team powered, `connectedPower`,
/// non-`PowerNode` building in each direction within `range` and stops at
/// insulated blocks. `beam-node` range 10 (`Blocks.java:2671`);
/// `plastanium-wall` is insulated (`Blocks.java`).
#[test]
fn beam_node_links_battery_in_line() {
    let mut sim = runtime_sim();
    let node = place(&mut sim, "beam-node", 5, 5);
    let battery = place(&mut sim, "battery", 12, 5);
    let graph = |sim: &Sim, e: Entity| {
        sim.ecs
            .0
            .get::<crate::world::modules::PowerModule>(e)
            .map(|module| module.graph)
    };
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        graph(&sim, node),
        graph(&sim, battery),
        "the beam node linked the battery 7 tiles in line"
    );

    // A power node is not a link target (`!(other.block instanceof PowerNode)`)
    // but it does not stop the scan: the beam still reaches the battery.
    let _power_node = place(&mut sim, "power-node", 8, 5);
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    assert_eq!(
        graph(&sim, node),
        graph(&sim, battery),
        "the power node is skipped, not treated as insulation"
    );

    // An insulated plastanium wall severs the line (`BeamNode.java:266`).
    let _wall = place(&mut sim, "plastanium-wall", 9, 5);
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    assert_ne!(
        graph(&sim, node),
        graph(&sim, battery),
        "the insulated wall broke the beam link"
    );
}
