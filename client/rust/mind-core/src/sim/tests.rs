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
            block: BlockId(999)
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
