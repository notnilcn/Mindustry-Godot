// SPDX-License-Identifier: GPL-3.0-only

//! Ordered delivery of the relay command log (plan 01 §3.8).
//!
//! [`CommandStream`] binds the `my_match_commands` view for one match and
//! applies rows in `command_id` order, never arrival order:
//!
//! - `command_id` is the global commit order; **numeric gaps are normal** and
//!   are never treated as loss (the counter is shared by all matches).
//! - duplicates (binder replay/reconnect snapshots) are ignored by
//!   `command_id`.
//! - loss/continuity is detected per sender with `sender_seq`
//!   ([`OrderError::Gap`]).
//!
//! Plan 21 consumes `drain()` at fixed-tick boundaries; this module performs no
//! command application itself.

use std::collections::{HashMap, HashSet};

use spacetimedb_sdk::Identity;

use crate::binder::{RowChange, TableBinder};
use crate::connector::Connector;
use crate::module_bindings::{MatchCommand, MyMatchCommandsTableAccessor};

/// A protocol-order violation surfaced instead of guessed around.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderError {
    /// A sender's `sender_seq` jumped (`got != expected`).
    Gap {
        /// Offending sender.
        sender: Identity,
        /// Expected next sequence.
        expected: u64,
        /// Received sequence.
        got: u64,
    },
    /// A late row carried a `command_id` below one already applied.
    Regression {
        /// Highest applied `command_id` before the late row.
        previous: u64,
        /// Late row's `command_id`.
        incoming: u64,
    },
}

/// Per-match ordered view over the relay command log.
pub struct CommandStream {
    binder: TableBinder<MyMatchCommandsTableAccessor>,
    match_id: u64,
    applied_ids: HashSet<u64>,
    last_command_id: u64,
    next_seq: HashMap<Identity, u64>,
    order_error: Option<OrderError>,
    applied_count: u64,
}

impl CommandStream {
    /// Subscribes the Game wave and binds `my_match_commands` for `match_id`.
    ///
    /// Bind early (before the Game wave applies) so the subscription's snapshot
    /// inserts reach the stream; reconnect snapshots are deduplicated.
    pub fn subscribe(connector: &mut Connector, match_id: u64) -> Self {
        connector.subscribe_game();
        let binder = connector.bind::<MyMatchCommandsTableAccessor>("my_match_commands");
        Self {
            binder,
            match_id,
            applied_ids: HashSet::new(),
            last_command_id: 0,
            next_seq: HashMap::new(),
            order_error: None,
            applied_count: 0,
        }
    }

    /// Match this stream is scoped to.
    pub fn match_id(&self) -> u64 {
        self.match_id
    }

    /// Drains and applies pending rows in `command_id` order.
    ///
    /// Returns only newly applied rows; duplicates and other matches' rows are
    /// dropped. Check [`CommandStream::order_error`] after every drain.
    pub fn drain(&mut self) -> Vec<MatchCommand> {
        let mut rows: Vec<MatchCommand> = self
            .binder
            .drain()
            .into_iter()
            .filter_map(|change| match change {
                RowChange::Insert(row) | RowChange::Update { new: row, .. } => Some(row),
                RowChange::Delete(_) => None,
            })
            .filter(|row| row.match_id == self.match_id)
            .collect();
        rows.sort_by_key(|row| row.command_id);

        let mut applied = Vec::new();
        for row in rows {
            if self.applied_ids.contains(&row.command_id) {
                continue;
            }
            if self.last_command_id > 0
                && row.command_id < self.last_command_id
                && self.order_error.is_none()
            {
                self.order_error = Some(OrderError::Regression {
                    previous: self.last_command_id,
                    incoming: row.command_id,
                });
            }
            self.applied_ids.insert(row.command_id);
            self.last_command_id = self.last_command_id.max(row.command_id);
            let expected = self.next_seq.entry(row.sender).or_insert(1);
            if row.sender_seq != *expected && self.order_error.is_none() {
                self.order_error = Some(OrderError::Gap {
                    sender: row.sender,
                    expected: *expected,
                    got: row.sender_seq,
                });
            }
            *expected = row.sender_seq.saturating_add(1);
            self.applied_count = self.applied_count.saturating_add(1);
            applied.push(row);
        }
        applied
    }

    /// Rows applied so far (committed commands observed in order).
    pub fn applied_count(&self) -> u64 {
        self.applied_count
    }

    /// Highest applied `command_id` (0 before the first command).
    pub fn last_command_id(&self) -> u64 {
        self.last_command_id
    }

    /// First order violation observed, if any.
    pub fn order_error(&self) -> Option<&OrderError> {
        self.order_error.as_ref()
    }

    /// Queued (not yet drained) changes in the binder.
    pub fn pending(&self) -> usize {
        self.binder.pending()
    }

    /// Synthetic delivery hook for the `stdb_command_order` scenario/tests.
    #[doc(hidden)]
    pub fn inject(&self, row: MatchCommand) {
        self.binder.inject(RowChange::Insert(row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConnectionConfig;
    use crate::module_bindings::CommandKind;
    use spacetimedb_sdk::Timestamp;

    fn command(id: u64, match_id: u64, sender: u8, seq: u64) -> MatchCommand {
        MatchCommand {
            command_id: id,
            match_id,
            sender: Identity::from_byte_array([sender; 32]),
            sender_seq: seq,
            client_tick: id,
            kind: CommandKind::Ping(id),
            sent_at: Timestamp::UNIX_EPOCH,
        }
    }

    fn stream(match_id: u64) -> CommandStream {
        let mut connector = Connector::new(ConnectionConfig::default());
        CommandStream::subscribe(&mut connector, match_id)
    }

    #[test]
    fn orders_by_command_id_not_arrival() {
        let mut stream = stream(7);
        stream.inject(command(9, 7, 1, 3));
        stream.inject(command(7, 7, 1, 1));
        stream.inject(command(8, 7, 1, 2));
        let rows = stream.drain();
        let ids: Vec<u64> = rows.iter().map(|row| row.command_id).collect();
        assert_eq!(ids, vec![7, 8, 9]);
        assert_eq!(stream.applied_count(), 3);
        assert_eq!(stream.last_command_id(), 9);
        assert_eq!(stream.order_error(), None);
    }

    #[test]
    fn duplicate_command_id_is_ignored() {
        let mut stream = stream(7);
        stream.inject(command(7, 7, 1, 1));
        stream.inject(command(7, 7, 1, 1));
        let rows = stream.drain();
        assert_eq!(rows.len(), 1);
        assert_eq!(stream.applied_count(), 1);
        assert_eq!(stream.order_error(), None);

        // Replay after reconnect: same row again.
        stream.inject(command(7, 7, 1, 1));
        stream.inject(command(8, 7, 1, 2));
        let rows = stream.drain();
        let ids: Vec<u64> = rows.iter().map(|row| row.command_id).collect();
        assert_eq!(ids, vec![8]);
    }

    #[test]
    fn per_sender_gap_is_flagged() {
        let mut stream = stream(7);
        stream.inject(command(1, 7, 1, 1));
        stream.inject(command(2, 7, 1, 3));
        stream.drain();
        match stream.order_error() {
            Some(OrderError::Gap { expected, got, .. }) => {
                assert_eq!(*expected, 2);
                assert_eq!(*got, 3);
            }
            other => panic!("expected Gap, got {other:?}"),
        }
    }

    #[test]
    fn autoincrement_numeric_gaps_are_not_loss() {
        let mut stream = stream(7);
        stream.inject(command(1, 7, 1, 1));
        stream.inject(command(40, 7, 1, 2));
        stream.inject(command(41, 7, 2, 1));
        let rows = stream.drain();
        assert_eq!(rows.len(), 3);
        assert_eq!(stream.order_error(), None);
    }

    #[test]
    fn other_matches_are_ignored() {
        let mut stream = stream(7);
        stream.inject(command(1, 8, 1, 1));
        stream.inject(command(2, 7, 1, 1));
        let rows = stream.drain();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].match_id, 7);
    }
}
