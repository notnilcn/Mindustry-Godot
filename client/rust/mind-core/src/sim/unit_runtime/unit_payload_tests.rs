// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit payloads (`world/blocks/payloads/UnitPayload.java`): carried-unit spawn,
//! `dump()` gates and the native nested payload body — GAPS #23 / PAR-0518.
//! Upstream: core/src/mindustry/entities/units/UnitPayload.java

use super::*;
use crate::content::BlockId;
use crate::sim::test_support::{install_live_runtimes, place_team1};

// ------------------------------------------------------------------
// Unit payloads (GAPS #23 / PAR-0518)
// ------------------------------------------------------------------

fn add_item(sim: &mut Sim, e: Entity, name: &str, amount: i32) {
    let id = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .item_id(name)
        .expect("item");
    if let Some(mut items) = sim.ecs.0.get_mut::<crate::world::modules::ItemModule>(e) {
        items.add(id, amount, 10_000);
    }
}

fn carried_unit_entity(sim: &Sim, holder: Entity) -> Option<Entity> {
    sim.held_payload(holder)
        .filter(|payload| !payload.is_block)
        .and_then(|payload| payload.entity)
}

/// GAPS #23 / PAR-0518: `UnitFactory.updateTile` builds a `UnitPayload`
/// instead of spawning the unit directly (`UnitFactory.java:424-444`); the
/// unit is carried (outside `Groups.unit`) until `moveOutPayload` dumps it,
/// and the dump enters it as a live unit with the planned type and health.
#[test]
fn unit_factory_output_is_carried_then_dumped() {
    let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    install_live_runtimes(&mut sim);
    let factory = place_team1(&mut sim, "ground-factory", 5, 5);
    let _power = place_team1(&mut sim, "power-source", 3, 5);
    add_item(&mut sim, factory, "silicon", 10);
    add_item(&mut sim, factory, "lead", 10);
    let dagger = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit_id("dagger")
        .expect("dagger");
    let dagger_health = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit(dagger)
        .expect("dagger def")
        .health;

    let mut carried = false;
    let mut dumped = false;
    for _ in 0..1400 {
        sim.tick().expect("tick");
        if let Some(entity) = carried_unit_entity(&sim, factory) {
            carried = true;
            let world = &sim.ecs.0;
            // A carried payload is not part of `Groups.unit`: no `Unit`
            // membership marker and not in the live runtime list.
            assert!(
                world.get::<crate::entities::comp::Unit>(entity).is_none(),
                "carried unit is not in Groups.unit"
            );
            assert!(!sim.unit_runtime().expect("runtime").units.contains(&entity));
            assert!(
                world
                    .get::<crate::world::blocks::payloads::CarriedUnit>(entity)
                    .is_some()
            );
            assert!(
                world
                    .get::<crate::world::blocks::payloads::PayloadCarried>(entity)
                    .is_some()
            );
            let type_id = world
                .get::<UnitTypeComp>(entity)
                .map(|comp| comp.type_id)
                .expect("unit type");
            assert_eq!(type_id, dagger, "the payload carries the planned unit");
            let health = world.get::<Health>(entity).map(|h| h.health).unwrap_or(0.0);
            assert_eq!(health, dagger_health, "payload spawns at full health");
        }
        if !dumped
            && sim
                .unit_runtime()
                .expect("runtime")
                .units
                .iter()
                .any(|entity| {
                    sim.ecs
                        .0
                        .get::<UnitTypeComp>(*entity)
                        .is_some_and(|comp| comp.type_id == dagger)
                })
        {
            dumped = true;
            break;
        }
    }
    assert!(carried, "the factory held its output as a unit payload");
    assert!(dumped, "moveOutPayload dumped the unit into the world");
    let live = sim
        .unit_runtime()
        .expect("runtime")
        .units
        .iter()
        .copied()
        .find(|entity| {
            sim.ecs
                .0
                .get::<UnitTypeComp>(*entity)
                .is_some_and(|comp| comp.type_id == dagger)
        })
        .expect("dumped unit tracked");
    assert!(
        sim.ecs.0.get::<crate::entities::comp::Unit>(live).is_some(),
        "the dumped unit rejoined Groups.unit"
    );
    assert_eq!(
        sim.ecs.0.get::<Health>(live).map(|h| h.health),
        Some(dagger_health)
    );
    assert!(
        carried_unit_entity(&sim, factory).is_none(),
        "the factory slot cleared after the dump"
    );
}

/// GAPS #23 / PAR-0518: `UnitPayload.dump` refuses a dump on a solid tile
/// or with a grounded unit of the same leg-step layer already overlapping
/// (`UnitPayload.java:104-127`), and succeeds once the blockers clear.
#[test]
fn unit_payload_dump_gates_solid_and_overlap() {
    let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    install_live_runtimes(&mut sim);
    let dagger = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit_id("dagger")
        .expect("dagger");
    let (x, y) = (16.0 * 8.0 + 4.0, 16.0 * 8.0 + 4.0);
    // A live grounded unit at the drop point: the overlap gate refuses.
    let live = sim
        .spawn_live_unit("dagger", 1, x, y, 0.0)
        .expect("live unit");
    let payload =
        crate::sim::unit_runtime::create_unit_payload(&mut sim.ecs.0, dagger, 1, x, y, 0.0)
            .expect("unit payload");
    let carried = payload.entity.expect("entity");
    assert!(
        !crate::sim::unit_runtime::dump_carried_unit(&mut sim.ecs.0, carried),
        "overlapping grounded unit refuses the dump"
    );
    // Clear the overlap: the dump succeeds and the unit becomes live.
    if let Some(mut pos) = sim.ecs.0.get_mut::<Pos>(live) {
        pos.x += 400.0;
    }
    assert!(
        crate::sim::unit_runtime::dump_carried_unit(&mut sim.ecs.0, carried),
        "cleared overlap dumps"
    );
    assert!(
        sim.ecs
            .0
            .get::<crate::entities::comp::Unit>(carried)
            .is_some()
    );
    assert!(
        sim.unit_runtime()
            .expect("runtime")
            .units
            .contains(&carried)
    );

    // A solid tile under the unit (`unit.solidity()` -> `Tile.solid`)
    // refuses the dump.
    let (x2, y2) = (20.0 * 8.0 + 4.0, 20.0 * 8.0 + 4.0);
    let payload2 =
        crate::sim::unit_runtime::create_unit_payload(&mut sim.ecs.0, dagger, 1, x2, y2, 0.0)
            .expect("unit payload");
    let carried2 = payload2.entity.expect("entity");
    let wall = sim.world_block_id("copper-wall").expect("wall");
    let tx = WorldGrid::to_tile(x2);
    let ty = WorldGrid::to_tile(y2);
    sim.grid.tiles.get_mut(tx, ty).block = BlockId::new(wall);
    sim.refresh_unit_runtime();
    assert!(
        !crate::sim::unit_runtime::dump_carried_unit(&mut sim.ecs.0, carried2),
        "solid tile refuses the dump"
    );
    sim.grid.tiles.get_mut(tx, ty).block = BlockId::AIR;
    sim.refresh_unit_runtime();
    assert!(
        crate::sim::unit_runtime::dump_carried_unit(&mut sim.ecs.0, carried2),
        "cleared solid dumps"
    );
}

/// GAPS #23 / PAR-0518: the native unit-payload body writes and reads the
/// carried unit (type/pos/rotation/health/team) so a payload-block save
/// round-trips.
#[test]
fn unit_payload_body_round_trips() {
    let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    install_live_runtimes(&mut sim);
    let dagger = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit_id("dagger")
        .expect("dagger");
    let payload = crate::sim::unit_runtime::create_unit_payload(
        &mut sim.ecs.0,
        dagger,
        1,
        100.0,
        120.0,
        37.0,
    )
    .expect("unit payload");
    let entity = payload.entity.expect("entity");
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(entity) {
        health.health = 42.0;
    }

    let mut bytes = Vec::new();
    {
        let mut writer = crate::io::wire::WireWriter::new(&mut bytes);
        crate::world::blocks::payloads::payload_write(&sim.ecs.0, Some(payload), &mut writer);
    }
    let mut reader = crate::io::wire::WireReader::new(&bytes);
    let decoded =
        crate::world::blocks::payloads::payload_block::payload_read(&mut sim.ecs.0, &mut reader);
    let crate::world::blocks::payloads::payload_block::PayloadRead::Unit(read_payload) = decoded
    else {
        panic!("unit payload decoded");
    };
    assert_eq!(read_payload.content, dagger.raw());
    let read_entity = read_payload.entity.expect("read entity");
    let world = &sim.ecs.0;
    assert_eq!(
        world.get::<UnitTypeComp>(read_entity).map(|c| c.type_id),
        Some(dagger)
    );
    assert_eq!(
        world.get::<Health>(read_entity).map(|h| h.health),
        Some(42.0),
        "health round-trips"
    );
    assert_eq!(
        world.get::<Pos>(read_entity).map(|pos| (pos.x, pos.y)),
        Some((100.0, 120.0))
    );
    assert_eq!(
        world.get::<UnitCore>(read_entity).map(|core| core.rotation),
        Some(37.0),
        "rotation round-trips"
    );
    assert!(
        world
            .get::<crate::world::blocks::payloads::CarriedUnit>(read_entity)
            .is_some(),
        "read unit is carried"
    );
    assert!(
        world
            .get::<crate::entities::comp::Unit>(read_entity)
            .is_none(),
        "read unit is not in Groups.unit"
    );
}
