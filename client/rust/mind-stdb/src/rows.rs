// SPDX-License-Identifier: GPL-3.0-only

//! Tiny hand-written row projections for the gdext debug surface (plan §3.5).
//!
//! Generated rows are SATS-serializable, not serde; the `StdbBinder` node only
//! needs a human/debug JSON string, so each supported row gets a small manual
//! projection here (generated code is never edited).

use crate::module_bindings::{
    MatchState, Player, ProtocolInfo, RelayConfig, RelayMatch, RelayMember,
};

/// Debug JSON for one row type.
pub trait RowView {
    /// Compact JSON object describing the row.
    fn debug_json(&self) -> String;
}

impl RowView for ProtocolInfo {
    fn debug_json(&self) -> String {
        serde_json::json!({
            "id": self.id,
            "protocol_version": self.protocol_version,
            "min_client_build": self.min_client_build,
            "save_format_version": self.save_format_version,
        })
        .to_string()
    }
}

impl RowView for RelayConfig {
    fn debug_json(&self) -> String {
        serde_json::json!({
            "id": self.id,
            "commands_per_second": self.commands_per_second,
            "command_rate_window_ms": self.command_rate_window_ms,
            "max_commit_commands_per_transaction": self.max_commit_commands_per_transaction,
            "default_map_width_tiles": self.default_map_width_tiles,
            "default_map_height_tiles": self.default_map_height_tiles,
        })
        .to_string()
    }
}

impl RowView for Player {
    fn debug_json(&self) -> String {
        serde_json::json!({
            "identity": self.identity.to_hex().to_string(),
            "username": self.username,
            "protocol_version": self.protocol_version,
            "last_seen_at": self.last_seen_at.to_micros_since_unix_epoch(),
        })
        .to_string()
    }
}

impl RowView for RelayMatch {
    fn debug_json(&self) -> String {
        serde_json::json!({
            "match_id": self.match_id,
            "map_id": self.map_id,
            "map_seed": self.map_seed,
            "map_width_tiles": self.map_width_tiles,
            "map_height_tiles": self.map_height_tiles,
            "status": format!("{:?}", self.status),
            "authority": format!("{:?}", self.authority),
            "protocol_version": self.protocol_version,
            "created_by": self.created_by.to_hex().to_string(),
            "host": self.host.to_hex().to_string(),
            "mode": format!("{:?}", self.mode),
            "mode_name": self.mode_name,
            "visibility": format!("{:?}", self.visibility),
            "rules_epoch": self.rules_epoch,
            "is_dedicated": self.is_dedicated,
            "player_count": self.player_count,
            "max_players": self.max_players,
            "last_command_id": self.last_command_id,
        })
        .to_string()
    }
}

impl RowView for RelayMember {
    fn debug_json(&self) -> String {
        serde_json::json!({
            "member_id": self.member_id,
            "match_id": self.match_id,
            "identity": self.identity.to_hex().to_string(),
            "role": format!("{:?}", self.role),
            "team": self.team,
            "connected": self.connected,
            "ready": self.ready,
            "kicked_reason": self.kicked_reason,
        })
        .to_string()
    }
}

impl RowView for MatchState {
    fn debug_json(&self) -> String {
        serde_json::json!({
            "state_id": self.state_id,
            "match_id": self.match_id,
            "wave": self.wave,
            "wavetime": self.wavetime,
            "enemies": self.enemies,
            "paused": self.paused,
            "game_over": self.game_over,
            "sim_tick": self.sim_tick,
            "last_command_id": self.last_command_id,
            "rules_epoch": self.rules_epoch,
        })
        .to_string()
    }
}
