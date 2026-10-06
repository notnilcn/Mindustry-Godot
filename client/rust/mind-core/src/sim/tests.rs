// SPDX-License-Identifier: GPL-3.0-only

//! P0-only simulation tests (no upstream equivalent except where noted).

use super::*;
use crate::command::CommandRecord;
use crate::content::BlockId;
use crate::scenario::{Scenario, ScenarioPlayer, command_log_to_string, parse_command_log};

/// Runs a command script and returns the sim plus the per-tick checksum stream.
fn run_scripted(seed: u64, records: &[CommandRecord], steps: u64) -> (Sim, Vec<u64>) {
    let mut sim = Sim::new(seed, 32, 32, BlockId::AIR, BlockId::AIR);
    let mut checksums = Vec::with_capacity(steps as usize);
    let mut next = 0usize;
    for tick in 0..steps {
        while next < records.len() && records[next].tick <= tick {
            sim.apply(records[next].command).unwrap();
            next += 1;
        }
        sim.tick().unwrap();
        checksums.push(sim.checksum());
    }
    (sim, checksums)
}

fn sample_records() -> Vec<CommandRecord> {
    vec![
        CommandRecord::new(
            0,
            Command::SelectBlock {
                block: BlockId::STONE_WALL,
            },
        ),
        CommandRecord::new(
            0,
            Command::Place {
                x: 4,
                y: 4,
                block: BlockId::STONE_WALL,
            },
        ),
        CommandRecord::new(
            1,
            Command::Place {
                x: 5,
                y: 4,
                block: BlockId::STONE_WALL,
            },
        ),
        CommandRecord::new(2, Command::Break { x: 4, y: 4 }),
        CommandRecord::new(
            7,
            Command::Place {
                x: 9,
                y: 9,
                block: BlockId::STONE_WALL,
            },
        ),
        CommandRecord::new(
            7,
            Command::Place {
                x: 10,
                y: 9,
                block: BlockId::STONE_WALL,
            },
        ),
    ]
}

#[test]
fn same_seed_same_checksums() {
    let records = sample_records();
    let (first, first_ticks) = run_scripted(42, &records, 30);
    let (second, second_ticks) = run_scripted(42, &records, 30);
    assert_eq!(first_ticks, second_ticks);
    assert_eq!(first.checksum(), second.checksum());
    assert_eq!(first.checksum_hex().len(), 16);

    let (other, _) = run_scripted(43, &records, 30);
    assert_ne!(
        first.checksum(),
        other.checksum(),
        "seed must affect the checksum"
    );
}

#[test]
fn command_log_replay_matches_direct_run() {
    let blocks = Blocks::new();
    let records = sample_records();

    let text = command_log_to_string(&blocks, &records).unwrap();
    let parsed = parse_command_log(&blocks, &text).unwrap();
    assert_eq!(parsed, records);

    let (direct, direct_ticks) = run_scripted(5, &records, 40);
    let (replayed, replay_ticks) = run_scripted(5, &parsed, 40);
    assert_eq!(direct_ticks, replay_ticks);
    assert_eq!(direct.checksum_hex(), replayed.checksum_hex());

    // A command that is stamped after `steps` never runs.
    let late = vec![CommandRecord::new(
        100,
        Command::Place {
            x: 0,
            y: 0,
            block: BlockId::STONE_WALL,
        },
    )];
    let (never, _) = run_scripted(5, &late, 40);
    assert_eq!(never.commands_applied(), 0);
}

#[test]
fn invalid_commands_are_noops_or_errors() {
    let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
    let place = Command::Place {
        x: 1,
        y: 1,
        block: BlockId::STONE_WALL,
    };
    sim.apply(place).unwrap();
    // Occupied tile: deterministic no-op, still counted.
    sim.apply(place).unwrap();
    assert_eq!(sim.commands_applied(), 2);
    assert_eq!(sim.block_at(TilePos::new(1, 1)), Some(BlockId::STONE_WALL));
    assert_eq!(sim.ecs.entities_by_seq().len(), 1);

    // Empty tile break: no-op.
    sim.apply(Command::Break { x: 3, y: 3 }).unwrap();
    assert_eq!(sim.commands_applied(), 3);

    // Bounds and content errors.
    assert!(matches!(
        sim.apply(Command::Place {
            x: 8,
            y: 0,
            block: BlockId::STONE_WALL
        }),
        Err(SimError::World(_))
    ));
    assert!(matches!(
        sim.apply(Command::Place {
            x: 0,
            y: 0,
            block: BlockId::new(999)
        }),
        Err(SimError::UnknownBlock(999))
    ));
    assert!(matches!(
        sim.apply(Command::Place {
            x: 0,
            y: 0,
            block: BlockId::AIR
        }),
        Err(SimError::CannotPlaceAir)
    ));

    // Break removes the entity and clears the tile.
    sim.apply(Command::Break { x: 1, y: 1 }).unwrap();
    assert_eq!(sim.block_at(TilePos::new(1, 1)), Some(BlockId::AIR));
    assert!(sim.ecs.entities_by_seq().is_empty());
}

#[test]
fn scenario_player_matches_stamped_ticks() {
    let scenario = Scenario::parse(
        r#"{
          "format": 1,
          "name": "player-test",
          "seed": 1,
          "world": { "generator": "flat", "width": 16, "height": 16, "floor": "stone", "wall": "air" },
          "commands": [
            { "tick": 0, "op": { "type": "place", "x": 1, "y": 1, "block": "stone-wall" } },
            { "tick": 2, "op": { "type": "place", "x": 2, "y": 1, "block": "stone-wall" } },
            { "tick": 2, "op": { "type": "break", "x": 1, "y": 1 } },
            { "tick": 4, "op": { "type": "place", "x": 3, "y": 1, "block": "stone-wall" } }
          ],
          "steps": 6
        }"#,
    )
    .unwrap();

    let mut sim = Sim::from_scenario(&scenario).unwrap();
    let mut player = ScenarioPlayer::new(&scenario, sim.content()).unwrap();
    assert_eq!(player.total_steps(), 6);
    player.run(&mut sim).unwrap();

    assert_eq!(sim.tick_count(), 6);
    assert_eq!(sim.block_at(TilePos::new(1, 1)), Some(BlockId::AIR));
    assert_eq!(sim.block_at(TilePos::new(2, 1)), Some(BlockId::STONE_WALL));
    assert_eq!(sim.block_at(TilePos::new(3, 1)), Some(BlockId::STONE_WALL));
    assert_eq!(player.applied(), 4);
    assert_eq!(sim.commands_applied(), 4);
}

#[test]
fn sim_command_applies_unit_command_and_control() {
    use crate::determinism::SimCommand;

    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    let id = sim.spawn_command_unit("dagger", 0).expect("dagger");
    assert!(sim.unit_commands().is_some());

    // Move order (emitter marker `command = 0`).
    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![id],
        command: 0,
        x: 50.0,
        y: 60.0,
    })
    .expect("unit command applies");
    assert_eq!(
        sim.unit_command_state(id).unwrap().target_pos,
        Some((50.0, 60.0))
    );

    // setUnitCommand (`repair` id 1 is rejected on a dagger; state unchanged).
    let before = sim.unit_command_state(id).unwrap().command;
    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![id],
        command: 1,
        x: 0.0,
        y: 0.0,
    })
    .expect("command switch applies as a no-op when disallowed");
    assert_eq!(sim.unit_command_state(id).unwrap().command, before);

    // setUnitStance carries unit ids on the wire and applies to each unit.
    let patrol = sim
        .unit_commands()
        .unwrap()
        .content()
        .unit_stance_by_name("patrol")
        .expect("patrol stance")
        .id;
    sim.command(SimCommand::UnitStance {
        units: smallvec::smallvec![id],
        stance: patrol.raw(),
        enabled: true,
    })
    .expect("unit stance applies");
    assert!(sim.unit_command_state(id).unwrap().stances.get(patrol));
    sim.command(SimCommand::UnitStance {
        units: smallvec::smallvec![id],
        stance: patrol.raw(),
        enabled: false,
    })
    .expect("unit stance clears");
    assert!(!sim.unit_command_state(id).unwrap().stances.get(patrol));

    // Player-control variants.
    sim.command(SimCommand::UnitControl { unit: Some(id) })
        .expect("unit control");
    assert_eq!(sim.controlled_unit(), Some(id));
    sim.command(SimCommand::BuildingControlSelect { x: 3, y: 4 })
        .expect("building control select");
    assert_eq!(sim.control_building(), Some(TilePos::new(3, 4)));
    sim.command(SimCommand::UnitClear).expect("unit clear");
    assert_eq!(sim.controlled_unit(), None);

    // move + command-switch + stance set + stance clear + unitControl +
    // buildingControlSelect + unitClear.
    assert_eq!(sim.commands_applied(), 7);
}

#[test]
fn sim_command_rotates_placed_building() {
    use crate::command::Command;
    use crate::determinism::{CommandError, SimCommand};

    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.apply(Command::Place {
        x: 3,
        y: 3,
        block: BlockId::STONE_WALL,
    })
    .expect("place");
    let entity = sim.grid.entity_at(TilePos::new(3, 3)).expect("building");
    let rot = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::ecs::BuildingComp>(entity)
            .unwrap()
            .rot
    };
    assert_eq!(rot(&sim), 0);

    // `true` = counter-clockwise (+1), `false` = clockwise (-1), mod 4.
    sim.command(SimCommand::Rotate {
        x: 3,
        y: 3,
        direction: true,
    })
    .expect("ccw");
    assert_eq!(rot(&sim), 1);
    sim.command(SimCommand::Rotate {
        x: 3,
        y: 3,
        direction: false,
    })
    .expect("cw");
    assert_eq!(rot(&sim), 0);
    sim.command(SimCommand::Rotate {
        x: 3,
        y: 3,
        direction: false,
    })
    .expect("cw wraps");
    assert_eq!(rot(&sim), 3);

    // `deletePlans` now applies over the relay `BuildQueue` (empty here).
    sim.command(SimCommand::DeletePlans {
        positions: smallvec::smallvec![1],
    })
    .expect("delete plans");

    // `Payload` stays blocked: the unit payload carrier (`Payloadc`) is not
    // ported, so there is no runtime to apply against.
    assert_eq!(
        sim.command(SimCommand::Payload {
            kind: 0,
            x: 0.0,
            y: 0.0,
            target: None,
        })
        .unwrap_err(),
        CommandError::Unsupported("payload")
    );
}

#[test]
fn sim_command_applies_inventory_transfers() {
    use crate::determinism::{CommandError, SimCommand};
    use crate::world::TilePos;

    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    // `router` holds items with a small capacity; resolve its real content id.
    let router = sim.world_block_id("router").expect("router resolves");
    sim.apply(Command::Place {
        x: 4,
        y: 4,
        block: BlockId::new(router),
    })
    .expect("place router");
    let runtime_item = sim.world_item_id("copper").expect("copper resolves");

    // Deposit from the player into the router; the capacity clamps it to 1.
    sim.set_player_item(Some((runtime_item, 2))).expect("seed");
    sim.command(SimCommand::Inventory {
        kind: 1,
        x: 4,
        y: 4,
        item: Some(runtime_item),
        amount: 5,
        angle: 0.0,
    })
    .expect("deposit");
    let pos = TilePos::new(4, 4);
    assert_eq!(
        sim.world_apply()
            .unwrap()
            .held_at(pos, crate::content::id::ItemId::new(runtime_item)),
        1,
        "router capacity clamps the deposit"
    );
    assert_eq!(sim.player_item(), Some((runtime_item, 1)));

    // Withdraw it back out.
    sim.command(SimCommand::Inventory {
        kind: 0,
        x: 4,
        y: 4,
        item: Some(runtime_item),
        amount: 1,
        angle: 0.0,
    })
    .expect("withdraw");
    assert_eq!(sim.player_item(), Some((runtime_item, 2)));

    // Drop clears the carried stack with no building effect.
    sim.command(SimCommand::Inventory {
        kind: 2,
        x: 4,
        y: 4,
        item: None,
        amount: 0,
        angle: 0.0,
    })
    .expect("drop");
    assert_eq!(sim.player_item(), None);

    // Unknown item ids are structured content errors, not panics.
    assert_eq!(
        sim.command(SimCommand::Inventory {
            kind: 1,
            x: 4,
            y: 4,
            item: Some(u16::MAX),
            amount: 1,
            angle: 0.0,
        })
        .unwrap_err(),
        CommandError::UnknownContent(u16::MAX)
    );
    // Out-of-bounds target.
    assert_eq!(
        sim.command(SimCommand::Inventory {
            kind: 0,
            x: 99,
            y: 0,
            item: Some(0),
            amount: 1,
            angle: 0.0,
        })
        .unwrap_err(),
        CommandError::InvalidTarget
    );
}

#[test]
fn sim_command_deletes_queued_plans() {
    use crate::content::BlockId;
    use crate::determinism::SimCommand;
    use crate::input::plan::ClientPlan;
    use crate::world::TilePos;

    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.enqueue_build_plan(ClientPlan::place(2, 3, 0, BlockId::STONE_WALL));
    sim.enqueue_build_plan(ClientPlan::place(5, 6, 0, BlockId::STONE_WALL));
    assert!(sim.build_plan_at(2, 3).is_some());

    sim.command(SimCommand::DeletePlans {
        positions: smallvec::smallvec![TilePos::new(2, 3).pack()],
    })
    .expect("delete plans");
    assert!(sim.build_plan_at(2, 3).is_none());
    assert!(sim.build_plan_at(5, 6).is_some(), "other plan survives");
}

#[test]
fn sim_command_commands_valid_buildings_only() {
    use crate::determinism::SimCommand;
    use crate::world::TilePos;

    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    let factory = sim
        .world_block_id("ground-factory")
        .expect("commandable block");
    sim.apply(Command::Place {
        x: 3,
        y: 3,
        block: BlockId::new(factory),
    })
    .expect("place factory");
    sim.apply(Command::Place {
        x: 6,
        y: 3,
        block: BlockId::STONE_WALL,
    })
    .expect("place wall");

    // Only the live, `Block.commandable` tile records a target: a wall and an
    // empty tile are both ignored (upstream `build.isCommandable()` gate).
    sim.command(SimCommand::CommandBuilding {
        positions: smallvec::smallvec![
            TilePos::new(3, 3).pack(),
            TilePos::new(6, 3).pack(),
            TilePos::new(8, 8).pack()
        ],
        x: 70.0,
        y: 42.0,
    })
    .expect("command buildings");
    assert_eq!(
        sim.building_command_target(TilePos::new(3, 3)),
        Some((70.0, 42.0))
    );
    assert_eq!(
        sim.building_command_target(TilePos::new(6, 3)),
        None,
        "a wall is not commandable"
    );
    assert_eq!(sim.building_command_target(TilePos::new(8, 8)), None);

    // A later command retargets the same building (append-only no; replaces).
    sim.command(SimCommand::CommandBuilding {
        positions: smallvec::smallvec![TilePos::new(3, 3).pack()],
        x: -1.0,
        y: 2.5,
    })
    .expect("retarget");
    assert_eq!(
        sim.building_command_target(TilePos::new(3, 3)),
        Some((-1.0, 2.5))
    );
}

#[test]
fn sim_command_world_apply_is_deterministic() {
    use crate::input::plan::ClientPlan;
    use crate::world::TilePos;

    fn build() -> Sim {
        let mut sim = Sim::new(7, 16, 16, BlockId::AIR, BlockId::AIR);
        let router = sim.world_block_id("router").expect("router resolves");
        sim.apply(Command::Place {
            x: 2,
            y: 2,
            block: BlockId::new(router),
        })
        .unwrap();
        let copper = sim.world_item_id("copper").expect("copper resolves");
        sim.set_player_item(Some((copper, 3))).unwrap();
        use crate::determinism::SimCommand;
        sim.command(SimCommand::Inventory {
            kind: 1,
            x: 2,
            y: 2,
            item: Some(copper),
            amount: 3,
            angle: 0.0,
        })
        .unwrap();
        sim.enqueue_build_plan(ClientPlan::place(4, 4, 0, BlockId::STONE_WALL));
        sim.command(SimCommand::DeletePlans {
            positions: smallvec::smallvec![TilePos::new(4, 4).pack()],
        })
        .unwrap();
        sim.command(SimCommand::CommandBuilding {
            positions: smallvec::smallvec![TilePos::new(2, 2).pack()],
            x: 9.0,
            y: 9.0,
        })
        .unwrap();
        sim
    }

    assert_eq!(
        build().world_apply_checksum(),
        build().world_apply_checksum()
    );
}

#[test]
fn sim_command_unit_state_is_deterministic() {
    use crate::determinism::SimCommand;

    let mut first = Sim::new(7, 16, 16, BlockId::AIR, BlockId::AIR);
    let mut second = Sim::new(7, 16, 16, BlockId::AIR, BlockId::AIR);
    for sim in [&mut first, &mut second] {
        let a = sim.spawn_command_unit("dagger", 0).expect("dagger");
        let b = sim.spawn_command_unit("dagger", 1).expect("dagger");
        sim.command(SimCommand::UnitCommand {
            units: smallvec::smallvec![a],
            command: 0,
            x: 12.0,
            y: 34.0,
        })
        .unwrap();
        sim.command(SimCommand::UnitCommand {
            units: smallvec::smallvec![b],
            command: 0,
            x: 9.0,
            y: 0.0,
        })
        .unwrap();
    }
    assert_eq!(
        first.unit_command_checksum(),
        second.unit_command_checksum()
    );
}

#[test]
fn block_runtime_provisions_and_ticks_placed_buildings() {
    use crate::ecs::BuildingComp;
    use crate::entities::comp::{Building, Health};
    use crate::world::block::BlockTable;
    use crate::world::update::BuildClock;

    let mut sim = Sim::new(7, 16, 16, BlockId::AIR, BlockId::AIR);
    assert!(!sim.has_block_runtime());
    sim.install_block_runtime().expect("runtime installs");
    assert!(sim.has_block_runtime());
    assert!(sim.ecs.0.contains_resource::<BlockTable>());
    assert!(
        sim.ecs
            .0
            .contains_resource::<crate::world::blocks::power::PowerGrids>()
    );

    let wall = sim.content().id("copper-wall").expect("copper-wall");
    sim.apply(Command::Place {
        x: 4,
        y: 4,
        block: wall,
    })
    .expect("place");
    let entity = sim
        .grid
        .entity_at(TilePos::new(4, 4))
        .expect("runtime entity");
    assert!(sim.ecs.0.get::<Building>(entity).is_some());
    assert!(sim.ecs.0.get::<BuildingComp>(entity).is_some());
    assert!(sim.ecs.0.get::<Health>(entity).is_some());

    let before = sim.ecs.0.get_resource::<BuildClock>().expect("clock").time;
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let after = sim.ecs.0.get_resource::<BuildClock>().expect("clock").time;
    assert!(after > before, "UpdateBuildings ran ({before} -> {after})");

    // `Command::Break` goes through the runtime tile ops and clears entity.
    sim.apply(Command::Break { x: 4, y: 4 }).expect("break");
    assert_eq!(sim.block_at(TilePos::new(4, 4)), Some(BlockId::AIR));
    assert!(sim.ecs.0.get_entity(entity).is_err());
}

#[test]
fn block_runtime_is_deterministic() {
    fn run() -> u64 {
        let mut sim = Sim::new(7, 32, 32, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("install");
        let wall = sim.content().id("copper-wall").expect("wall");
        let conveyor = sim
            .block_runtime()
            .expect("runtime")
            .content()
            .block_id("conveyor")
            .expect("conveyor");
        for (x, y) in [(2, 2), (3, 2), (4, 4)] {
            sim.apply(Command::Place { x, y, block: wall })
                .expect("place wall");
        }
        sim.command(crate::determinism::SimCommand::Place {
            x: 6,
            y: 6,
            block: conveyor.raw(),
            rotation: 1,
            team: 0,
            player: None,
        })
        .expect("place conveyor");
        for _ in 0..10 {
            sim.tick().expect("tick");
        }
        sim.runtime_checksum().value()
    }
    assert_eq!(run(), run());
}

#[test]
fn block_runtime_honors_place_rotation_end_to_end() {
    use crate::determinism::SimCommand;
    use crate::entities::comp::Building;

    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("install");
    let conveyor = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("conveyor")
        .expect("conveyor");
    sim.command(SimCommand::Place {
        x: 3,
        y: 3,
        block: conveyor.raw(),
        rotation: 1,
        team: 0,
        player: None,
    })
    .expect("rotated place");
    let entity = sim.grid.entity_at(TilePos::new(3, 3)).expect("building");
    assert_eq!(
        sim.ecs
            .0
            .get::<Building>(entity)
            .expect("runtime building")
            .rotation,
        1
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::ecs::BuildingComp>(entity)
            .expect("p0 comp")
            .rot,
        1
    );

    // A rotated `Place` is not representable as a P0 command: it must not be
    // silently dropped by `to_p0`.
    let rotated = SimCommand::Place {
        x: 3,
        y: 3,
        block: conveyor.raw(),
        rotation: 2,
        team: 0,
        player: None,
    };
    assert!(rotated.to_p0().is_none());
    assert!(
        SimCommand::Place {
            x: 3,
            y: 3,
            block: conveyor.raw(),
            rotation: 0,
            team: 0,
            player: None,
        }
        .to_p0()
        .is_some()
    );
}

#[test]
fn block_runtime_configure_applies_to_building() {
    use crate::determinism::{ConfigValue as Wire, SimCommand};
    use crate::world::config::{ConfigValue, read_config};

    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("install");
    let sorter = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("sorter")
        .expect("sorter");
    sim.apply(Command::Place {
        x: 3,
        y: 3,
        block: BlockId::new(sorter.raw()),
    })
    .expect("place sorter");

    sim.command(SimCommand::Configure {
        x: 3,
        y: 3,
        value: Wire::Content(String::from("copper")),
    })
    .expect("configure sorter");
    let entity = sim.grid.entity_at(TilePos::new(3, 3)).expect("building");
    let copper = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .item_by_name("copper")
        .expect("copper")
        .id;
    assert_eq!(read_config(&sim.ecs.0, entity), ConfigValue::Item(copper));

    // Unknown content and empty tiles are structured errors.
    assert_eq!(
        sim.command(SimCommand::Configure {
            x: 3,
            y: 3,
            value: Wire::Content(String::from("not-a-content")),
        })
        .unwrap_err(),
        crate::determinism::CommandError::UnknownContent(u16::MAX)
    );
    assert_eq!(
        sim.command(SimCommand::Configure {
            x: 9,
            y: 9,
            value: Wire::Bool(true),
        })
        .unwrap_err(),
        crate::determinism::CommandError::InvalidTarget
    );
}

#[test]
fn block_runtime_power_graph_updates_in_schedule() {
    use crate::world::blocks::power::{PowerGrids, PowerNodeInfo, PowerProduction};
    use crate::world::modules::PowerModule;

    let mut sim = Sim::new(3, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("install");
    let node = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("power-node")
        .expect("power-node");
    sim.apply(Command::Place {
        x: 4,
        y: 4,
        block: BlockId::new(node.raw()),
    })
    .expect("place a");
    sim.apply(Command::Place {
        x: 5,
        y: 4,
        block: BlockId::new(node.raw()),
    })
    .expect("place b");
    let a = sim.grid.entity_at(TilePos::new(4, 4)).expect("a");
    let b = sim.grid.entity_at(TilePos::new(5, 4)).expect("b");
    sim.ecs.0.entity_mut(a).insert(PowerNodeInfo::producer());
    sim.ecs.0.entity_mut(a).insert(PowerProduction(1.0));
    sim.ecs
        .0
        .entity_mut(b)
        .insert(PowerNodeInfo::consumer(100.0));
    sim.rebuild_power_graphs();
    {
        let graphs = sim.ecs.0.get_resource::<PowerGrids>().expect("power arena");
        assert_eq!(graphs.graph_count(), 1, "adjacent nodes share one graph");
    }

    let status = |sim: &Sim| sim.ecs.0.get::<PowerModule>(b).expect("module").status;
    assert_eq!(status(&sim), 0.0);
    sim.tick().expect("tick");
    let after = status(&sim);
    assert!(
        after > 0.0 && after < 1.0,
        "UpdatePowerGraph ran: status={after}"
    );
}

#[test]
fn block_runtime_replaces_compatible_occupied_tile() {
    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("install");
    let wall = sim.content().id("copper-wall").expect("copper-wall");
    let upgraded = sim.content().id("titanium-wall").expect("titanium-wall");
    sim.apply(Command::Place {
        x: 2,
        y: 2,
        block: wall,
    })
    .expect("place");
    sim.apply(Command::Place {
        x: 2,
        y: 2,
        block: upgraded,
    })
    .expect("replace");
    assert_eq!(sim.block_at(TilePos::new(2, 2)), Some(upgraded));
    let entity = sim.grid.entity_at(TilePos::new(2, 2)).expect("entity");
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::entities::comp::Building>(entity)
            .expect("runtime building")
            .block,
        upgraded
    );
    let runtime = sim.block_runtime().expect("runtime");
    assert_eq!(runtime.counter().count(0, wall), 0);
    assert_eq!(runtime.counter().count(0, upgraded), 1);
}

#[test]
fn block_runtime_inventory_targets_live_item_module() {
    use crate::determinism::SimCommand;
    use crate::world::modules::ItemModule;

    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("install");
    let container = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("container")
        .expect("container");
    sim.apply(Command::Place {
        x: 4,
        y: 4,
        block: BlockId::new(container.raw()),
    })
    .expect("place container");
    let entity = sim.grid.entity_at(TilePos::new(4, 4)).expect("entity");
    let copper = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .item_by_name("copper")
        .expect("copper")
        .id;

    sim.set_player_item(Some((copper.raw(), 10))).expect("seed");
    sim.command(SimCommand::Inventory {
        kind: 1,
        x: 4,
        y: 4,
        item: Some(copper.raw()),
        amount: 6,
        angle: 0.0,
    })
    .expect("deposit");
    assert_eq!(
        sim.ecs
            .0
            .get::<ItemModule>(entity)
            .expect("module")
            .get(copper),
        6
    );
    assert_eq!(sim.player_item(), Some((copper.raw(), 4)));

    sim.command(SimCommand::Inventory {
        kind: 0,
        x: 4,
        y: 4,
        item: Some(copper.raw()),
        amount: 2,
        angle: 0.0,
    })
    .expect("withdraw");
    assert_eq!(
        sim.ecs
            .0
            .get::<ItemModule>(entity)
            .expect("module")
            .get(copper),
        4
    );
    assert_eq!(sim.player_item(), Some((copper.raw(), 6)));

    sim.command(SimCommand::Inventory {
        kind: 2,
        x: 4,
        y: 4,
        item: None,
        amount: 0,
        angle: 0.0,
    })
    .expect("drop");
    assert_eq!(sim.player_item(), None);
}

#[test]
fn block_runtime_placement_gate_rejects_locked_and_special_visibility() {
    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime_with(SimRuntimeConfig {
        placement_gate: true,
        campaign: true,
    })
    .expect("install");
    let source = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("power-source")
        .expect("power-source");
    let shown = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("canvas")
        .expect("shown block");

    // Sandbox-only content is gated even in a campaign.
    sim.apply(Command::Place {
        x: 2,
        y: 2,
        block: BlockId::new(source.raw()),
    })
    .expect("rejected no-op");
    assert_eq!(sim.block_at(TilePos::new(2, 2)), Some(BlockId::AIR));

    // A shown but locked block is gated until unlocked.
    sim.apply(Command::Place {
        x: 3,
        y: 3,
        block: BlockId::new(shown.raw()),
    })
    .expect("locked no-op");
    assert_eq!(sim.block_at(TilePos::new(3, 3)), Some(BlockId::AIR));
    sim.block_runtime_mut()
        .expect("runtime")
        .content_mut()
        .block_mut(shown)
        .expect("shown def")
        .unlock
        .unlocked = true;
    sim.apply(Command::Place {
        x: 3,
        y: 3,
        block: BlockId::new(shown.raw()),
    })
    .expect("unlocked place");
    assert_eq!(
        sim.block_at(TilePos::new(3, 3)),
        Some(BlockId::new(shown.raw()))
    );
}
