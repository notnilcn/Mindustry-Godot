// SPDX-License-Identifier: GPL-3.0-only

//! The per-frame drains: relay commands, the outbox, player states and checksums.
//! Upstream: core/src/mindustry/net/NetClient.java

use super::adapters::{MemberOrigin, SimHostContent};
use super::*;

impl MindNet {
    pub(super) fn drain_relay(&mut self) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        let commands = {
            let Some(host) = self.sim_host.as_ref() else {
                return;
            };
            let guard = host.bind();
            let content = SimHostContent(&guard);
            let default_team = guard.player_team();
            let origin = MemberOrigin {
                members: &self.members,
                default_team,
            };
            runtime.drain(&content, &origin)
        };
        self.queue_depth = runtime.queue_depth() as i64;
        self.last_checksum = self
            .sim_host
            .as_ref()
            .map(|host| host.bind().get_checksum().to_string())
            .unwrap_or_default();
        if let Some(host) = self.sim_host.as_mut() {
            let mut host = host.bind_mut();
            for command in commands {
                host.enqueue_sim_command(command);
            }
        }
    }

    /// Drains the player-command mailbox into the relay (I08 T6): each command
    /// maps to a relay kind, `Immediate` kinds are also applied locally as
    /// predictions, and everything sends through the match `CommandSender`.
    pub(super) fn drain_outbox(&mut self) {
        let Some(outbox) = self.relay_outbox.as_ref() else {
            return;
        };
        let commands: Vec<mind_core::determinism::SimCommand> = {
            let Ok(mut queue) = outbox.lock() else {
                return;
            };
            std::mem::take(&mut *queue)
        };
        for command in commands {
            self.send_player_command(command);
        }
    }

    /// Relays one player action (`MindNet` side of the single outbound seam).
    pub(super) fn send_player_command(&mut self, command: mind_core::determinism::SimCommand) {
        let kind = {
            let Some(host) = self.sim_host.as_ref() else {
                return;
            };
            let guard = host.bind();
            let content = SimHostContent(&guard);
            relay::sim_to_kind(&command, &content)
        };
        let Some(kind) = kind else {
            // No relay equivalent (`ConfigTap`, `SpawnUnit`): apply locally and
            // report the kind once (I08 T6).
            let name = command.op_name().to_string();
            if self.unmapped_kinds.insert(name.clone()) {
                log::info!("MindNet: `{name}` has no relay equivalent; applied locally only");
            }
            if let Some(host) = self.sim_host.as_mut() {
                host.bind_mut().enqueue_sim_command(command);
            }
            return;
        };
        if mind_stdb::commands::predict_policy(&kind) == mind_stdb::PredictPolicy::Immediate {
            // Prediction: the action applies locally now; the echo advances the
            // watermark (I08 T6).
            if let Some(host) = self.sim_host.as_mut() {
                host.bind_mut().enqueue_sim_command(command);
            }
        }
        let tick = self.sim_tick();
        let Some(connector) = self.connector.as_mut() else {
            return;
        };
        let Some(sender) = self.sender.as_mut() else {
            return;
        };
        let mut transport = StdbTransport::new(connector);
        match sender.send(&mut transport, tick, kind) {
            Ok(seq) => {
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.on_prediction(seq);
                }
            }
            Err(error) => {
                log::warn!("MindNet.send_player_command failed: {error}");
            }
        }
    }

    /// Emits one relay command with prediction bookkeeping (plan §3.5).
    pub(super) fn emit_command(&mut self, kind: CommandKind) -> bool {
        let tick = self.sim_tick();
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        let Some(sender) = self.sender.as_mut() else {
            return false;
        };
        let mut transport = StdbTransport::new(connector);
        match sender.send(&mut transport, tick, kind) {
            Ok(seq) => {
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.on_prediction(seq);
                }
                true
            }
            Err(error) => {
                log::warn!("MindNet.emit_command failed: {error}");
                false
            }
        }
    }

    /// Mirrors `my_match_player_states` rows for the renderer/tests (plan M3).
    pub(super) fn drain_player_states(&mut self) {
        let Some(binder) = self.players_binder.as_ref() else {
            return;
        };
        for change in binder.drain() {
            if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change {
                self.remote_players
                    .insert(row.identity.to_hex().to_string(), row);
            }
        }
    }

    /// Feeds `my_match_checksums` rows into the desync monitor (plan M4).
    pub(super) fn drain_checksums(&mut self) {
        let Some(binder) = self.checksums_binder.as_ref() else {
            return;
        };
        let host = self.session.host_identity();
        if let Some(host) = host {
            self.checksum_monitor.set_host(host);
        }
        for change in binder.drain() {
            if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change {
                self.checksum_monitor.record(ChecksumReport {
                    sender: row.sender,
                    command_id: row.command_id,
                    sim_tick: row.sim_tick,
                    checksum: row.checksum,
                    checksum_version: row.checksum_version,
                    scope: row.scope,
                });
                self.last_checksum = format!("{:016x}", row.checksum);
            }
        }
        if self.checksum_monitor.detect().is_some() && self.session.resync_count() == 0 {
            log::warn!("MindNet: desync detected; requesting a host snapshot");
            let _ = self.request_snapshot();
        }
    }
}
