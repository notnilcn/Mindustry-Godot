// SPDX-License-Identifier: GPL-3.0-only

//! Local command ring buffer for snapshot tail replay (plan 21 §3.7).
//!
//! Every applied relay command is retained (bounded by [`REPLAY_TAIL`]) so a
//! client entering
//! `SnapshotSync` can restore the host snapshot at `command_id = C` and replay
//! every command with `command_id > C` locally, without waiting for a second
//! snapshot. Missing tail entries are the caller's signal to request a snapshot
//! at the current `last_command_id` instead.

use std::collections::VecDeque;

use crate::module_bindings::{CommandKind, MatchCommand};

/// Default ring capacity (plan §3.7 `REPLAY_TAIL`).
pub const REPLAY_TAIL: usize = 4096;

/// One retained command (subset of `MatchCommand` needed for replay).
#[derive(Debug, Clone, PartialEq)]
pub struct RingEntry {
    /// Global commit order.
    pub command_id: u64,
    /// Per-sender sequence.
    pub sender_seq: u64,
    /// Sender identity (echo skip on replay).
    pub sender: spacetimedb_sdk::Identity,
    /// Command variant.
    pub kind: CommandKind,
}

impl From<&MatchCommand> for RingEntry {
    fn from(row: &MatchCommand) -> Self {
        Self {
            command_id: row.command_id,
            sender_seq: row.sender_seq,
            sender: row.sender,
            kind: row.kind.clone(),
        }
    }
}

/// Bounded FIFO of applied commands.
#[derive(Debug, Clone)]
pub struct CommandRing {
    entries: VecDeque<RingEntry>,
    cap: usize,
}

impl Default for CommandRing {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRing {
    /// Ring with [`REPLAY_TAIL`] capacity.
    pub fn new() -> Self {
        Self::with_capacity(REPLAY_TAIL)
    }

    /// Ring with an explicit capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    /// Retains `row`, evicting the oldest entry past capacity. Idempotent by
    /// `command_id` (reconnect replay re-delivers rows).
    pub fn push(&mut self, row: &MatchCommand) -> bool {
        if self
            .entries
            .back()
            .map(|last| row.command_id <= last.command_id)
            .unwrap_or(false)
            && self.entries.iter().any(|e| e.command_id == row.command_id)
        {
            return false;
        }
        self.entries.push_back(RingEntry::from(row));
        while self.entries.len() > self.cap {
            self.entries.pop_front();
        }
        true
    }

    /// Entries strictly newer than `command_id`, in order.
    pub fn tail_after(&self, command_id: u64) -> Vec<RingEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.command_id > command_id)
            .cloned()
            .collect()
    }

    /// Whether the ring covers the contiguous range `(command_id, through]`.
    ///
    /// A pruned hole makes tail replay impossible and forces a fresh snapshot.
    pub fn covers_tail(&self, command_id: u64, through: u64) -> bool {
        if through <= command_id {
            return true;
        }
        let mut next = command_id + 1;
        for entry in &self.entries {
            if entry.command_id < next {
                continue;
            }
            if entry.command_id != next {
                return false;
            }
            next += 1;
            if next > through {
                return true;
            }
        }
        false
    }

    /// Oldest retained `command_id`.
    pub fn oldest(&self) -> Option<u64> {
        self.entries.front().map(|entry| entry.command_id)
    }

    /// Newest retained `command_id`.
    pub fn newest(&self) -> Option<u64> {
        self.entries.back().map(|entry| entry.command_id)
    }

    /// Entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the ring is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drops every entry (resync/reconnect).
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use spacetimedb_sdk::{Identity, Timestamp};

    fn row(id: u64, seq: u64) -> MatchCommand {
        MatchCommand {
            command_id: id,
            match_id: 1,
            sender: Identity::from_byte_array([1u8; 32]),
            sender_seq: seq,
            client_tick: id,
            kind: CommandKind::Ping(id),
            sent_at: Timestamp::UNIX_EPOCH,
        }
    }

    #[test]
    fn tail_replay_window() {
        let mut ring = CommandRing::with_capacity(4);
        for id in 1..=10 {
            ring.push(&row(id, id));
        }
        // Capacity evicted 1..=6.
        assert_eq!(ring.oldest(), Some(7));
        assert_eq!(ring.newest(), Some(10));
        assert_eq!(
            ring.tail_after(8)
                .iter()
                .map(|entry| entry.command_id)
                .collect::<Vec<_>>(),
            vec![9, 10]
        );
        assert!(ring.covers_tail(7, 10));
        assert!(!ring.covers_tail(1, 10));
        // Duplicates from reconnect replay are ignored.
        assert!(!ring.push(&row(10, 10)));
        assert_eq!(ring.len(), 4);
    }

    #[test]
    fn gap_breaks_tail_coverage() {
        let mut ring = CommandRing::new();
        ring.push(&row(5, 1));
        ring.push(&row(6, 2));
        ring.push(&row(8, 3));
        assert!(ring.covers_tail(5, 6));
        // 7 is missing, so a restore at 4 cannot reach 8 by replay.
        assert!(!ring.covers_tail(4, 8));
    }
}
