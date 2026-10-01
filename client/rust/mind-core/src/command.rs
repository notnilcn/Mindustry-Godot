// SPDX-License-Identifier: GPL-3.0-only

//! Commands and the tick-stamped command log.
//!
//! Ported from `core/src/mindustry/input/Placement.java` / `BuildPlan` intent
//! (P0 subset; plan 07 replaces the command handling, plan 21 makes the command
//! log the multiplayer wire format).

use crate::content::BlockId;

/// A single player/script intent applied to the simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Place a block at `(x, y)`.
    Place { x: i16, y: i16, block: BlockId },
    /// Break the block at `(x, y)`.
    Break { x: i16, y: i16 },
    /// Change the currently selected block (UI intent, no world effect).
    SelectBlock { block: BlockId },
}

impl Command {
    /// Human/JSON op name.
    pub const fn op_name(&self) -> &'static str {
        match self {
            Command::Place { .. } => "place",
            Command::Break { .. } => "break",
            Command::SelectBlock { .. } => "select_block",
        }
    }
}

/// A command stamped with the tick it must be applied before (§6.1/§6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandRecord {
    /// Simulation tick the command belongs to.
    pub tick: u64,
    /// The command itself.
    pub command: Command,
}

impl CommandRecord {
    /// Creates a tick-stamped record.
    pub const fn new(tick: u64, command: Command) -> Self {
        Self { tick, command }
    }
}

/// Sorts records by tick with a *stable* sort, preserving file order for equal
/// ticks (§6.1).
pub fn sort_records(records: &mut [CommandRecord]) {
    records.sort_by_key(|record| record.tick);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_sort_preserves_equal_tick_order() {
        let mut records = vec![
            CommandRecord::new(2, Command::Break { x: 0, y: 0 }),
            CommandRecord::new(1, Command::Break { x: 1, y: 1 }),
            CommandRecord::new(2, Command::Break { x: 2, y: 2 }),
            CommandRecord::new(1, Command::Break { x: 3, y: 3 }),
        ];
        sort_records(&mut records);
        let ticks: Vec<u64> = records.iter().map(|record| record.tick).collect();
        assert_eq!(ticks, vec![1, 1, 2, 2]);
        let xs: Vec<i16> = records
            .iter()
            .map(|record| match record.command {
                Command::Break { x, .. } => x,
                _ => -1,
            })
            .collect();
        assert_eq!(xs, vec![1, 3, 0, 2]);
    }
}
