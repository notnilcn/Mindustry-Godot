// SPDX-License-Identifier: GPL-3.0-only

//! Canonical JSON state dump.
//!
//! Format per `00_FOUNDATION_IMPLEMENTATION_PLAN.md` §6.3: structs/`BTreeMap` only,
//! world tiles sparse (non-air) by default and sorted by `(y, x)`, entities sorted
//! by `EntitySeq`, `checksum` always present.

use serde::{Deserialize, Serialize};

use crate::content::BlockId;
use crate::event::SimEvent;
use crate::game::State;
use crate::sim::Sim;
use crate::version::{APP_NAME, DUMP_FORMAT, MIND_VERSION};

/// Root of the state dump (`format: 1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateDump {
    /// Dump format (`version::DUMP_FORMAT`).
    pub format: u32,
    /// Application name.
    pub app: String,
    /// `mind-core` version.
    pub version: String,
    /// Completed ticks.
    pub tick: u64,
    /// Simulation seed.
    pub seed: u64,
    /// Current phase (`menu`/`playing`/`paused`).
    pub phase: State,
    /// Final checksum, 16 lowercase hex digits.
    pub checksum: String,
    /// World section.
    pub world: DumpWorld,
    /// Placed entities, sorted by entity sequence.
    pub entities: Vec<DumpEntity>,
    /// Block events, oldest first.
    pub events: Vec<DumpEvent>,
    /// Number of commands applied.
    pub commands_applied: u64,
}

/// World section of the dump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpWorld {
    /// Width in tiles.
    pub width: i32,
    /// Height in tiles.
    pub height: i32,
    /// Whether `tiles` only lists non-air tiles.
    pub sparse: bool,
    /// Tiles, row-major sorted by `(y, x)`.
    pub tiles: Vec<DumpTile>,
}

/// One tile entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpTile {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Block name (`air` for empty tiles emitted with `--all-tiles`).
    pub block: String,
    /// Team id.
    pub team: u8,
    /// Rotation.
    pub rot: u8,
    /// `EntitySeq` of the building entity, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_id: Option<u64>,
}

/// One entity entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpEntity {
    /// Entity sequence id.
    pub id: u64,
    /// Entity kind (`building` at P0).
    pub kind: String,
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Block name.
    pub block: String,
}

/// One event entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpEvent {
    /// Tick the event was flushed at.
    pub tick: u64,
    /// Event kind (`block_placed`/`block_broken` at P0).
    pub kind: String,
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Block name.
    pub block: String,
}

/// Builds the canonical dump from a sim.
pub(crate) fn build(sim: &Sim, all_tiles: bool) -> StateDump {
    let mut tiles = Vec::new();
    for (pos, index) in sim.grid.iter_row_major() {
        let block = sim.grid.blocks[index];
        let entity = sim.grid.tiles[index];
        if !all_tiles && block == BlockId::AIR && entity.is_none() {
            continue;
        }
        tiles.push(DumpTile {
            x: pos.x(),
            y: pos.y(),
            block: sim.block_name_of(block),
            team: sim.grid.teams[index],
            rot: sim.grid.rots[index],
            build_id: entity.and_then(|handle| sim.ecs.seq_of(handle)),
        });
    }

    let entities = sim
        .ecs
        .entities_by_seq()
        .into_iter()
        .map(|(id, _entity, comp)| DumpEntity {
            id,
            kind: String::from("building"),
            x: comp.pos.x(),
            y: comp.pos.y(),
            block: sim.block_name_of(comp.block),
        })
        .collect();

    StateDump {
        format: DUMP_FORMAT,
        app: APP_NAME.to_owned(),
        version: MIND_VERSION.to_owned(),
        tick: sim.tick_count(),
        seed: sim.seed(),
        phase: sim.phase(),
        checksum: sim.checksum_hex(),
        world: DumpWorld {
            width: sim.grid.width,
            height: sim.grid.height,
            sparse: !all_tiles,
            tiles,
        },
        entities,
        events: sim.event_log().to_vec(),
        commands_applied: sim.commands_applied(),
    }
}

/// Converts a queued block event into its dump record (non-block events return `None`).
pub(crate) fn block_event_record(sim: &Sim, event: &SimEvent) -> Option<DumpEvent> {
    let tick = sim.state.tick;
    match event {
        SimEvent::BlockPlacedEvent(event) => Some(DumpEvent {
            tick,
            kind: String::from("block_placed"),
            x: event.x,
            y: event.y,
            block: sim.block_name_of(event.block),
        }),
        SimEvent::BlockBrokenEvent(event) => Some(DumpEvent {
            tick,
            kind: String::from("block_broken"),
            x: event.x,
            y: event.y,
            block: sim.block_name_of(event.block),
        }),
        SimEvent::ClientCreateEvent(_)
        | SimEvent::ClientLoadEvent(_)
        | SimEvent::ContentInitEvent(_)
        | SimEvent::StateChangeEvent(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::content::BlockId;

    #[test]
    fn json_roundtrip() {
        let mut sim = Sim::new(11, 8, 8, BlockId::AIR, BlockId::AIR);
        sim.apply(Command::Place {
            x: 2,
            y: 3,
            block: BlockId::STONE_WALL,
        })
        .unwrap();
        sim.apply(Command::Place {
            x: 5,
            y: 1,
            block: BlockId::STONE_WALL,
        })
        .unwrap();
        sim.tick().unwrap();
        sim.apply(Command::Break { x: 5, y: 1 }).unwrap();
        sim.tick().unwrap();

        let dump = sim.dump();
        let json = serde_json::to_string_pretty(&dump).unwrap();
        let decoded: StateDump = serde_json::from_str(&json).unwrap();
        assert_eq!(dump, decoded);

        assert_eq!(dump.format, DUMP_FORMAT);
        assert_eq!(dump.phase, State::Playing);
        assert_eq!(dump.tick, 2);
        assert_eq!(dump.world.width, 8);
        assert!(dump.world.sparse);
        assert_eq!(dump.world.tiles.len(), 1);
        assert_eq!(dump.world.tiles[0].block, "stone-wall");
        assert_eq!((dump.world.tiles[0].x, dump.world.tiles[0].y), (2, 3));
        assert_eq!(dump.entities.len(), 1);
        assert_eq!(dump.entities[0].id, 0);
        assert_eq!(dump.events.len(), 3);
        assert_eq!(dump.events[0].kind, "block_placed");
        assert_eq!(dump.events[2].kind, "block_broken");
        assert_eq!(dump.commands_applied, 3);
    }

    #[test]
    fn dump_is_byte_stable_across_runs() {
        let build = || {
            let mut sim = Sim::new(3, 16, 16, BlockId::AIR, BlockId::AIR);
            for x in 0..4 {
                sim.apply(Command::Place {
                    x,
                    y: 2,
                    block: BlockId::STONE_WALL,
                })
                .unwrap();
            }
            for _ in 0..10 {
                sim.tick().unwrap();
            }
            sim.dump_json(true).unwrap()
        };
        assert_eq!(build(), build());
    }

    #[test]
    fn dump_honours_all_tiles() {
        let sim = Sim::new(1, 4, 4, BlockId::AIR, BlockId::AIR);
        let sparse = sim.dump();
        assert!(sparse.world.tiles.is_empty());
        let full = sim.dump_all_tiles();
        assert_eq!(full.world.tiles.len(), 16);
        assert!(!full.world.sparse);
        assert!(full.world.tiles.iter().all(|tile| tile.block == "air"));
    }
}
