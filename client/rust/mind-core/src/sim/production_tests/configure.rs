// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime configure-tap and point-config regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;

/// PAR-0190: wire `ContentId`/`Bool` configs reach `configured()` — sorter,
/// door (with Java's 60-tick toggle throttle) and switch.
#[test]
fn configure_reaches_sorter_door_switch() {
    use crate::content::ContentType;
    use crate::entities::comp::{Building, DoorState};

    let mut sim = runtime_sim();
    let sorter = place(&mut sim, "sorter", 5, 5);
    let door = place(&mut sim, "door", 8, 5);
    let switch = place(&mut sim, "switch", 11, 5);
    let copper = item(&sim, "copper");

    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::ContentId {
            kind: ContentType::Item as u8,
            id: copper.raw(),
        },
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::world::blocks::distribution::SorterBuild>(sorter)
            .and_then(|sorter| sorter.sort_item),
        Some(copper),
        "typed item id reached Sorter.configured"
    );

    configure(&mut sim, 8, 5, crate::determinism::ConfigValue::Bool(true));
    assert!(
        sim.ecs
            .0
            .get::<DoorState>(door)
            .is_some_and(|state| state.open),
        "door opened"
    );
    for _ in 0..61 {
        sim.tick().expect("tick");
    }
    configure(&mut sim, 8, 5, crate::determinism::ConfigValue::Bool(false));
    assert!(
        sim.ecs
            .0
            .get::<DoorState>(door)
            .is_some_and(|state| !state.open),
        "door closed after the toggle throttle elapsed"
    );

    configure(
        &mut sim,
        11,
        5,
        crate::determinism::ConfigValue::Bool(false),
    );
    assert!(
        !sim.ecs.0.get::<Building>(switch).expect("switch").enabled,
        "switch disabled"
    );
}

/// PAR-0190: an absolute packed `Integer` config links a bridge conveyor and
/// the sim keeps ticking (the reported `configure_building(..,"int",packed)`
/// hang; `ItemBridge.java:66-69`).
#[test]
fn bridge_configure_int_absolute_links_and_ticks() {
    use crate::world::blocks::distribution::ItemBridgeBuild;
    use crate::world::modules::ItemModule;

    let mut sim = runtime_sim();
    let from = place(&mut sim, "bridge-conveyor", 5, 5);
    let _to = place(&mut sim, "bridge-conveyor", 8, 5);
    let belt = place(&mut sim, "conveyor", 9, 5);
    let copper = item(&sim, "copper");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemModule>(from) {
        items.add(copper, 10, 10);
    }
    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::Int(TilePos::new(8, 5).pack()),
    );
    assert_eq!(
        sim.ecs.0.get::<ItemBridgeBuild>(from).map(|b| b.link),
        Some(TilePos::new(8, 5).pack()),
        "absolute packed int links the bridge"
    );
    // Must terminate: the G03 hang was on the next step after this configure.
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let dest = sim.grid.entity_at(TilePos::new(8, 5)).expect("dest bridge");
    let belt_items = sim
        .ecs
        .0
        .get::<ItemModule>(belt)
        .map(|module| module.get(copper))
        .unwrap_or(0);
    assert!(
        stored_item(&sim, dest, copper) + belt_items > 0,
        "copper arrived at (8,5) or beyond (dest {}, belt {belt_items})",
        stored_item(&sim, dest, copper)
    );
}

/// PAR-0190: a relative `Point2` config links `(5,5)` to `(8,5)`
/// (`ItemBridge.config(Point2.class)`, `ItemBridge.java:66-69`); the old bridge
/// packed it as an absolute position and linked to tile (3,0).
#[test]
fn bridge_configure_point_relative() {
    use crate::world::blocks::distribution::ItemBridgeBuild;

    let mut sim = runtime_sim();
    let from = place(&mut sim, "bridge-conveyor", 5, 5);
    let _to = place(&mut sim, "bridge-conveyor", 8, 5);
    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::Point2(3, 0),
    );
    assert_eq!(
        sim.ecs.0.get::<ItemBridgeBuild>(from).map(|b| b.link),
        Some(TilePos::new(8, 5).pack())
    );
}

/// PAR-0190: a `Point2[]` (relative points) config autolinks a power node
/// (`PowerNode.config(Point2[].class)`, `PowerNode.java:97-109`).
#[test]
fn node_configure_point_array() {
    use crate::world::modules::PowerModule;

    let mut sim = runtime_sim();
    let node = place(&mut sim, "power-node", 5, 5);
    let _battery_a = place(&mut sim, "battery", 8, 5);
    let _battery_b = place(&mut sim, "battery", 5, 8);
    configure(
        &mut sim,
        5,
        5,
        crate::determinism::ConfigValue::Point2Array(vec![
            TilePos::new(3, 0).pack(),
            TilePos::new(0, 3).pack(),
        ]),
    );
    let links = sim
        .ecs
        .0
        .get::<PowerModule>(node)
        .map(|module| module.links.len())
        .unwrap_or(0);
    assert_eq!(links, 2, "node linked both batteries");
}

/// H02 T6: `SimCommand::ConfigTap` runs `config_tap` and applies the returned
/// configs in order (bridge tap-to-link through the sim command path).
#[test]
fn sim_config_tap_command_applies() {
    use crate::sim::ConfigTapKind;
    use crate::world::blocks::distribution::ItemBridgeBuild;

    let mut sim = runtime_sim();
    let from = place(&mut sim, "bridge-conveyor", 5, 5);
    let _to = place(&mut sim, "bridge-conveyor", 8, 5);
    sim.command(SimCommand::ConfigTap {
        x: 5,
        y: 5,
        other_x: 8,
        other_y: 5,
    })
    .expect("config tap");
    assert_eq!(sim.last_config_tap, Some(ConfigTapKind::Consume));
    assert_eq!(
        sim.ecs.0.get::<ItemBridgeBuild>(from).map(|b| b.link),
        Some(TilePos::new(8, 5).pack())
    );
    // Inverse tap: the reverse-link branch switches and clears the old link.
    sim.command(SimCommand::ConfigTap {
        x: 8,
        y: 5,
        other_x: 5,
        other_y: 5,
    })
    .expect("config tap");
    assert_eq!(sim.last_config_tap, Some(ConfigTapKind::Switch));
    assert_eq!(
        sim.ecs.0.get::<ItemBridgeBuild>(from).map(|b| b.link),
        Some(-1)
    );
    let to = sim.grid.entity_at(TilePos::new(8, 5)).expect("dest");
    assert_eq!(
        sim.ecs.0.get::<ItemBridgeBuild>(to).map(|b| b.link),
        Some(TilePos::new(5, 5).pack())
    );
}
