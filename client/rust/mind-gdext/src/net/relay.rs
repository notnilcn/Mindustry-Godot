// SPDX-License-Identifier: GPL-3.0-only

//! `RelayRuntime` — drains the ordered relay log and maps foreign rows to
//! `mind-core` [`SimCommand`]s (plan 21 §3.5, §6.3).
//!
//! Applied only at fixed-tick boundaries by `MindNet` (via
//! `MindSimHost::enqueue_sim_command`); never mid-frame. The client's own
//! echoed rows are skipped (the action was already applied as a prediction),
//! but they still advance the `applied_command_id` watermark.

use std::collections::VecDeque;

use mind_core::determinism::SimCommand;
use mind_stdb::module_bindings::{CommandKind, MatchCommand};
use mind_stdb::{CommandStream, Connector, Identity};

/// Maps a relay variant to a canonical sim command, if it affects the sim.
///
/// Plan 21 M2 covers the world intents (`PlaceBlock`/`BreakBlock`/`ConfigBlock`);
/// the remaining variants are accepted by the schema and wired in later
/// milestones (the match is `predict_policy`-classified in `mind-stdb`).
pub fn kind_to_sim(kind: &CommandKind) -> Option<SimCommand> {
    match kind {
        CommandKind::PlaceBlock(payload) => Some(SimCommand::Place {
            x: i16::try_from(payload.x).ok()?,
            y: i16::try_from(payload.y).ok()?,
            block: block_id(&payload.block),
            rotation: payload.rotation as i8,
            team: 0,
            player: None,
        }),
        CommandKind::BreakBlock(payload) => Some(SimCommand::Break {
            x: i16::try_from(payload.x).ok()?,
            y: i16::try_from(payload.y).ok()?,
            player: None,
        }),
        _ => None,
    }
}

/// Content-name → id mirror (plan 02 owns the real catalog; M2 relay tests use
/// the spine's two known names, everything else maps to id 0).
fn block_id(name: &str) -> u16 {
    match name {
        "router" => 1,
        _ => 0,
    }
}

/// Per-match relay consumer (echo skip + foreign apply mapping).
pub struct RelayRuntime {
    stream: CommandStream,
    local: Option<Identity>,
    applied_command_id: u64,
    in_flight: VecDeque<u64>,
    applied: u64,
}

impl RelayRuntime {
    /// Subscribes the Game wave and binds `my_match_commands`.
    pub fn subscribe(connector: &mut Connector, match_id: u64) -> Self {
        Self {
            stream: CommandStream::subscribe(connector, match_id),
            local: None,
            applied_command_id: 0,
            in_flight: VecDeque::new(),
            applied: 0,
        }
    }

    /// Records the local identity so own echoes can be skipped.
    pub fn set_local_identity(&mut self, identity: Identity) {
        self.local = Some(identity);
    }

    /// Records a locally predicted command's client sequence.
    pub fn on_prediction(&mut self, sender_seq: u64) {
        self.in_flight.push_back(sender_seq);
    }

    /// Highest applied `command_id` (checkpoint watermark).
    pub fn last_applied_command_id(&self) -> u64 {
        self.applied_command_id
    }

    /// Rows applied from the log (foreign + own echoes advanced).
    pub fn applied_count(&self) -> u64 {
        self.applied
    }

    /// Queued (not yet drained) changes.
    pub fn queue_depth(&self) -> usize {
        self.stream.pending()
    }

    /// First stream order violation, as a diagnostic string.
    pub fn order_error(&self) -> Option<String> {
        self.stream.order_error().map(|error| format!("{error:?}"))
    }

    /// Drains rows and returns foreign commands to apply at the next tick.
    ///
    /// Own echoes are skipped (prediction already applied) but advance the
    /// watermark.
    pub fn drain(&mut self) -> Vec<SimCommand> {
        let rows: Vec<MatchCommand> = self.stream.drain();
        let mut out = Vec::new();
        for row in rows {
            self.applied_command_id = self.applied_command_id.max(row.command_id);
            self.applied = self.applied.saturating_add(1);
            if Some(row.sender) == self.local {
                // Own echo: one prediction resolved (best-effort FIFO).
                self.in_flight.pop_front();
                continue;
            }
            if let Some(command) = kind_to_sim(&row.kind) {
                out.push(command);
            }
        }
        out
    }
}
