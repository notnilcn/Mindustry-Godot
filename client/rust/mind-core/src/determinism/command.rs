// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SimCommand` / `CommandLog` — the transport-free command interface
//! (plan 05 §6.4).
//!
//! Plan 21 turns STDB `command_event` rows into `SimCommand`s in relay order;
//! plan 23 replays `.simlog` files. The text form is JSON Lines: one header line
//! then one command per line.

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::command::Command as P0Command;
use crate::content::BlockId;

/// A configuration value payload (`@Configure` intent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConfigValue {
    /// No value.
    None,
    /// Boolean.
    Bool(bool),
    /// Integer.
    Int(i32),
    /// Float.
    Float(f32),
    /// Content reference by name (parity ABI).
    Content(String),
    /// Raw byte payload (mod extension).
    Bytes(Vec<u8>),
}

/// A single simulation command (plan 05 §6.4).
///
/// `SetRules` carries a JSON blob until plan 12's typed `Rules` lands
/// (documented deferral); plan 21/12 replace it in place without changing the
/// other variants.
#[derive(Debug, Clone, PartialEq)]
pub enum SimCommand {
    /// Place a block.
    Place {
        /// Tile x.
        x: i16,
        /// Tile y.
        y: i16,
        /// Block content id.
        block: u16,
        /// Rotation.
        rotation: i8,
        /// Team id.
        team: u8,
        /// Issuing player, when known.
        player: Option<i32>,
    },
    /// Break a block.
    Break {
        /// Tile x.
        x: i16,
        /// Tile y.
        y: i16,
        /// Issuing player, when known.
        player: Option<i32>,
    },
    /// Configure a building.
    Configure {
        /// Tile x.
        x: i16,
        /// Tile y.
        y: i16,
        /// Configuration value.
        value: ConfigValue,
    },
    /// Issue a unit command.
    UnitCommand {
        /// Unit entity ids.
        units: SmallVec<[i32; 32]>,
        /// Command content id.
        command: u16,
        /// Target x.
        x: f32,
        /// Target y.
        y: f32,
    },
    /// Replace the match rules (plan 12 owns the typed payload).
    SetRules {
        /// Serialized rules JSON.
        json: String,
    },
    /// Spawn a unit (server/harness only).
    SpawnUnit {
        /// Unit content id.
        unit: u16,
        /// X.
        x: f32,
        /// Y.
        y: f32,
        /// Team id.
        team: u8,
    },
    /// Mod/data extension.
    Custom {
        /// Extension kind.
        kind: u16,
        /// Extension data.
        data: SmallVec<[u8; 32]>,
    },
}

impl SimCommand {
    /// Parity op name.
    pub const fn op_name(&self) -> &'static str {
        match self {
            SimCommand::Place { .. } => "place",
            SimCommand::Break { .. } => "break",
            SimCommand::Configure { .. } => "configure",
            SimCommand::UnitCommand { .. } => "unit_command",
            SimCommand::SetRules { .. } => "set_rules",
            SimCommand::SpawnUnit { .. } => "spawn_unit",
            SimCommand::Custom { .. } => "custom",
        }
    }

    /// Converts the P0 subset commands into `SimCommand`s.
    pub fn from_p0(command: P0Command) -> Self {
        match command {
            P0Command::Place { x, y, block } => SimCommand::Place {
                x,
                y,
                block: block.raw(),
                rotation: 0,
                team: 0,
                player: None,
            },
            P0Command::Break { x, y } => SimCommand::Break { x, y, player: None },
            P0Command::SelectBlock { block } => SimCommand::Configure {
                x: 0,
                y: 0,
                value: ConfigValue::Content(format!("{}", block.raw())),
            },
        }
    }

    /// Converts back to the P0 subset when representable.
    pub fn to_p0(&self) -> Option<P0Command> {
        match self {
            SimCommand::Place { x, y, block, .. } => Some(P0Command::Place {
                x: *x,
                y: *y,
                block: BlockId::new(*block),
            }),
            SimCommand::Break { x, y, .. } => Some(P0Command::Break { x: *x, y: *y }),
            _ => None,
        }
    }
}

/// Log header (plan 05 §6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogHeader {
    /// Format version.
    pub format: u32,
    /// Master seed.
    pub seed: u64,
    /// Map name.
    pub map: String,
    /// Map content hash.
    pub map_hash: u64,
    /// Build tag.
    pub build: String,
    /// Content registry hash.
    pub content_hash: u64,
}

impl LogHeader {
    /// Current `.simlog` format version.
    pub const FORMAT: u32 = 1;

    /// Creates a header for a build/seed/map.
    pub fn new(seed: u64, map: impl Into<String>) -> Self {
        Self {
            format: Self::FORMAT,
            seed,
            map: map.into(),
            map_hash: 0,
            build: crate::version::MIND_VERSION.to_owned(),
            content_hash: 0,
        }
    }
}

/// The recorded command stream.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandLog {
    /// Header.
    pub header: LogHeader,
    /// Tick-stamped commands in application order.
    pub entries: Vec<(u64, SimCommand)>,
}

impl CommandLog {
    /// Creates an empty log.
    pub fn new(header: LogHeader) -> Self {
        Self {
            header,
            entries: Vec::new(),
        }
    }

    /// Appends a command at `tick`.
    pub fn push(&mut self, tick: u64, command: SimCommand) {
        self.entries.push((tick, command));
    }

    /// Number of commands.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the log has no commands.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Commands for exactly `tick`, in insertion order.
    pub fn commands_at(&self, tick: u64) -> impl Iterator<Item = &SimCommand> {
        self.entries
            .iter()
            .filter(move |(entry_tick, _)| *entry_tick == tick)
            .map(|(_, command)| command)
    }
}

/// Command application errors (structured; never panics).
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// The command is not representable in this build.
    #[error("unsupported command `{0}`")]
    Unsupported(&'static str),
    /// The command referenced an unknown block/content id.
    #[error("unknown content id {0}")]
    UnknownContent(u16),
    /// The command target was invalid.
    #[error("invalid command target")]
    InvalidTarget,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p0_roundtrip_for_place_and_break() {
        let place = SimCommand::from_p0(P0Command::Place {
            x: 3,
            y: 4,
            block: BlockId::STONE_WALL,
        });
        assert_eq!(place.op_name(), "place");
        assert_eq!(
            place.to_p0(),
            Some(P0Command::Place {
                x: 3,
                y: 4,
                block: BlockId::STONE_WALL
            })
        );
        let brk = SimCommand::from_p0(P0Command::Break { x: 1, y: 2 });
        assert_eq!(brk.to_p0(), Some(P0Command::Break { x: 1, y: 2 }));
        assert!(
            SimCommand::SetRules {
                json: "{}".to_owned()
            }
            .to_p0()
            .is_none()
        );
    }

    #[test]
    fn command_log_orders_by_tick() {
        let mut log = CommandLog::new(LogHeader::new(7, "flat"));
        log.push(
            2,
            SimCommand::Break {
                x: 0,
                y: 0,
                player: None,
            },
        );
        log.push(
            1,
            SimCommand::Break {
                x: 1,
                y: 1,
                player: None,
            },
        );
        log.push(
            2,
            SimCommand::Break {
                x: 2,
                y: 2,
                player: None,
            },
        );
        assert_eq!(log.len(), 3);
        let at_two: Vec<&SimCommand> = log.commands_at(2).collect();
        assert_eq!(at_two.len(), 2);
        assert_eq!(log.header.build, crate::version::MIND_VERSION);
    }
}
