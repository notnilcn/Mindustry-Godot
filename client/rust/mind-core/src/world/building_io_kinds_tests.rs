// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! G04 T3a: per-kind codec byte-layout tests. Each test hand-builds the Java
//! body (field order from the cited `write`), reads it through the block's
//! registered behavior and asserts the write side reproduces the same bytes.

use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World as EcsWorld;

use crate::content::test_support::test_registry;
use crate::io::wire::{WireReader, WireWriter};
use crate::world::block::{BlockInstance, BlockTable};
use crate::world::limits::BuildRules;
use crate::world::{TilePos, WorldGrid};

fn spawn_block(name: &str) -> (EcsWorld, Entity, Arc<BlockInstance>) {
    let content = test_registry();
    let table = BlockTable::build_default(&content).expect("table");
    let inst = table
        .get_named(name)
        .unwrap_or_else(|| panic!("{name}"))
        .clone();
    let mut world = EcsWorld::new();
    world.insert_resource(BuildRules::default());
    world.insert_resource(crate::world::modules::ModuleDims {
        items: content.items().len(),
        liquids: content.liquids().len(),
    });
    world.insert_resource(table);
    let entity = inst.spawn(
        &mut world,
        0,
        TilePos::new(3, 3),
        0,
        0,
        content.items().len(),
        content.liquids().len(),
    );
    inst.behavior.create_state(&mut world, entity);
    (world, entity, inst)
}

/// Reads `bytes` through the behavior, asserts full consumption, then asserts
/// the write side reproduces the same bytes.
fn assert_body_roundtrip(
    world: &mut EcsWorld,
    inst: &Arc<BlockInstance>,
    e: Entity,
    bytes: &[u8],
    revision: u8,
) {
    let mut reader = WireReader::new(bytes);
    inst.behavior.read(world, e, &mut reader, revision);
    assert_eq!(
        reader.remaining(),
        0,
        "Java body fully consumed ({} left)",
        reader.remaining()
    );
    let mut out = Vec::new();
    {
        let mut writer = WireWriter::new(&mut out);
        inst.behavior.write(world, e, &mut writer);
    }
    assert_eq!(out, bytes, "write matches the Java bytes");
}

#[test]
fn force_projector_body_matches_java() {
    use crate::world::blocks::defense::shields::ForceProjectorState;

    let (mut world, e, inst) = spawn_block("force-projector");
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        w.bool(true);
        w.f(12.5);
        w.f(0.75);
        w.f(1.0);
        w.f(0.25);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 0);
    let state = world.get::<ForceProjectorState>(e).expect("state");
    assert!(state.broken);
    assert_eq!(state.buildup, 12.5);
    assert_eq!(state.radscl, 0.75);
    assert_eq!(state.warmup, 1.0);
    assert_eq!(state.phase_heat, 0.25);
}

#[test]
fn mend_projector_body_matches_java() {
    use crate::world::blocks::defense::shields::MendProjectorState;

    let (mut world, e, inst) = spawn_block("mend-projector");
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        w.f(5.0);
        w.f(0.125);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 0);
    let state = world.get::<MendProjectorState>(e).expect("state");
    assert_eq!(state.heat, 5.0);
    assert_eq!(state.phase_heat, 0.125);
}

#[test]
fn overdrive_projector_body_matches_java() {
    use crate::world::blocks::defense::projectors::OverdriveProjectorState;

    let (mut world, e, inst) = spawn_block("overdrive-projector");
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        w.f(2.5);
        w.f(0.5);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 0);
    let state = world.get::<OverdriveProjectorState>(e).expect("state");
    assert_eq!(state.heat, 2.5);
    assert_eq!(state.phase_heat, 0.5);
}

#[test]
fn base_shield_body_matches_java() {
    use crate::world::blocks::defense::projectors::BaseShieldState;

    let (mut world, e, inst) = spawn_block("shield-projector");
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        w.f(3.5);
        w.bool(true);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 1);
    let state = world.get::<BaseShieldState>(e).expect("state");
    assert_eq!(state.smooth_radius, 3.5);
    assert!(state.broken);
}

#[test]
fn shield_wall_body_matches_java() {
    use crate::world::blocks::defense::shields::ShieldWallState;

    let (mut world, e, inst) = spawn_block("shielded-wall");
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        w.f(25.0);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 0);
    let state = world.get::<ShieldWallState>(e).expect("state");
    assert_eq!(state.shield, 25.0);
    // `if(shield > 0) shieldRadius = 1f` (`ShieldWall.java:129`).
    assert_eq!(state.shield_radius, 1.0);
}

#[test]
fn radar_body_matches_java() {
    use crate::entities::comp::RadarState;

    let (mut world, e, inst) = spawn_block("radar");
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        w.f(0.5);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 0);
    let state = world.get::<RadarState>(e).expect("state");
    assert_eq!(state.progress, 0.5);
}

#[test]
fn stack_conveyor_body_matches_java() {
    use crate::content::ItemId;
    use crate::world::blocks::distribution::stack_conveyor::StackConveyorBuild;
    use crate::world::modules::ItemModule;

    let (mut world, e, inst) = spawn_block("plastanium-conveyor");
    {
        let mut items = world.get_mut::<ItemModule>(e).expect("items");
        items.add(ItemId::COPPER, 3, 10);
    }
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        w.i(7);
        w.f(1.25);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 0);
    let state = world.get::<StackConveyorBuild>(e).expect("state");
    assert_eq!(state.link, 7);
    assert_eq!(state.cooldown, 1.25);
    // `lastItem = items.first()` (`StackConveyor.java:402`).
    assert_eq!(state.last_item, Some(ItemId::COPPER));
}

#[test]
fn beam_drill_body_matches_java() {
    use crate::entities::comp::DrillState;

    let (mut world, e, inst) = spawn_block("plasma-bore");
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        w.f(100.0);
        w.f(1.0);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 1);
    let state = world.get::<DrillState>(e).expect("state");
    assert_eq!(state.progress, 100.0);
    assert_eq!(state.warmup, 1.0);
}

#[test]
fn combustion_generator_body_matches_java() {
    use crate::world::blocks::power::generator::GeneratorState;

    let (mut world, e, inst) = spawn_block("combustion-generator");
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        w.f(0.5);
        w.f(37.0);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 1);
    let state = world.get::<GeneratorState>(e).expect("state");
    assert_eq!(state.production_efficiency, 0.5);
    assert_eq!(state.generate_time, 37.0);
}

#[test]
fn thorium_reactor_body_matches_java() {
    use crate::world::blocks::power::behavior::NuclearReactorState;
    use crate::world::blocks::power::generator::GeneratorState;

    let (mut world, e, inst) = spawn_block("thorium-reactor");
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        w.f(1.0);
        w.f(10.0);
        w.f(0.42);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 1);
    let generator = world.get::<GeneratorState>(e).expect("generator");
    assert_eq!(generator.production_efficiency, 1.0);
    assert_eq!(generator.generate_time, 10.0);
    let state = world.get::<NuclearReactorState>(e).expect("state");
    assert_eq!(state.heat, 0.42);
}

#[test]
fn impact_reactor_body_matches_java() {
    use crate::world::blocks::power::behavior::ImpactReactorState;
    use crate::world::blocks::power::generator::GeneratorState;

    let (mut world, e, inst) = spawn_block("impact-reactor");
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        w.f(0.25);
        w.f(2.0);
        w.f(0.75);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 1);
    let generator = world.get::<GeneratorState>(e).expect("generator");
    assert_eq!(generator.production_efficiency, 0.25);
    assert_eq!(generator.generate_time, 2.0);
    let state = world.get::<ImpactReactorState>(e).expect("state");
    assert_eq!(state.warmup, 0.75);
}

#[test]
fn heat_producer_body_matches_java() {
    use crate::entities::comp::CrafterState;
    use crate::world::blocks::heat::HeatState;

    let (mut world, e, inst) = spawn_block("electric-heater");
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        w.f(0.4);
        w.f(0.6);
        w.f(8.0);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 0);
    let crafter = world.get::<CrafterState>(e).expect("crafter");
    assert_eq!(crafter.progress, 0.4);
    assert_eq!(crafter.warmup, 0.6);
    let heat = world.get::<HeatState>(e).expect("heat");
    assert_eq!(heat.heat, 8.0);
}

#[test]
fn cultivator_body_has_legacy_warmup() {
    use crate::entities::comp::CrafterState;

    let (mut world, e, inst) = spawn_block("cultivator");
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        w.f(0.2);
        w.f(0.3);
        // `legacyReadWarmup` trailing zero (`GenericCrafter.java:382`).
        w.f(0.0);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 0);
    let state = world.get::<CrafterState>(e).expect("state");
    assert_eq!(state.progress, 0.2);
    assert_eq!(state.warmup, 0.3);
}

/// T3b: a build payload on a payload conveyor survives the Java
/// `Payload.write`/`read` round trip (`Payload.java:87-118`,
/// `PayloadConveyor.java:314-330`).
#[test]
fn payload_conveyor_keeps_build_payload() {
    use crate::entities::comp::Building;
    use crate::world::blocks::payloads::{PayloadHolder, PayloadRef, create_build_payload};
    use crate::world::building_io::{DecodedBase, apply_body, read_base_legacy, write_all_java};

    let content = test_registry();
    let wall = content.block_id("copper-wall").expect("copper-wall");
    let (mut world, src, inst) = spawn_block("payload-conveyor");
    let payload = create_build_payload(&mut world, wall, 0).expect("payload");
    {
        let mut holder = world.get_mut::<PayloadHolder>(src).expect("holder");
        holder.payload = Some(PayloadRef {
            entity: Some(payload),
            content: wall.raw(),
            is_block: true,
        });
    }
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        write_all_java(&world, src, &mut w, false).expect("write");
    }
    // A fresh conveyor reads the chunk back.
    let (items, liquids) = world
        .get_resource::<crate::world::modules::ModuleDims>()
        .map(|dims| (dims.items, dims.liquids))
        .unwrap_or((0, 0));
    let dst = inst.spawn(&mut world, 1, TilePos::new(4, 4), 0, 0, items, liquids);
    inst.behavior.create_state(&mut world, dst);
    let mut r = WireReader::new(&bytes);
    let revision = r.ub().expect("kind revision");
    let mut decoded = DecodedBase::default();
    read_base_legacy(&mut decoded, &mut r).expect("read base");
    decoded.kind_revision = Some(revision);
    decoded.body = r.bytes(r.remaining()).expect("body").to_vec();
    crate::world::building_io::apply_base(&mut world, dst, &decoded, 100.0);
    apply_body(&mut world, dst, &decoded);

    let holder = world.get::<PayloadHolder>(dst).expect("holder");
    let payload = holder.payload.expect("payload restored");
    assert!(payload.is_block);
    let block = payload
        .entity
        .and_then(|entity| world.get::<Building>(entity))
        .map(|building| building.block);
    assert_eq!(block, Some(wall), "copper-wall payload restored");
}

/// T3b: the unit factory body starts with the PayloadBlock prefix
/// (`UnitFactory.java:486-506`); the hand-built Java bytes decode fully.
#[test]
fn unit_factory_body_matches_java() {
    use crate::world::blocks::units::UnitFactoryBuild;

    let (mut world, e, inst) = spawn_block("ground-factory");
    let mut bytes = Vec::new();
    {
        let mut w = WireWriter::new(&mut bytes);
        // PayloadBlock prefix: payVector (0,0), payRotation 0, no payload.
        w.f(0.0);
        w.f(0.0);
        w.f(0.0);
        w.bool(false);
        // UnitFactory body: progress 12.5, plan 0, vecNullable, command.
        w.f(12.5);
        w.s(0);
        w.f(f32::NAN);
        w.f(f32::NAN);
        w.ub(255);
    }
    assert_body_roundtrip(&mut world, &inst, e, &bytes, 3);
    let state = world.get::<UnitFactoryBuild>(e).expect("state");
    assert_eq!(state.progress, 12.5);
    assert_eq!(state.current_plan, 0);
}

/// The smoke helper also asserts the `WorldGrid` fixture compiles in this file.
#[allow(dead_code)]
fn _grid_fixture() -> WorldGrid {
    WorldGrid::new(0, 0)
}
