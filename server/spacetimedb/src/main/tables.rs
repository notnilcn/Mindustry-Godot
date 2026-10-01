// SPDX-License-Identifier: GPL-3.0-only

//! Module-wide singleton tables: protocol handshake and relay configuration
//! (plan 01 §6.1). Both are seeded in [`super::seeds`] on `init`.

use spacetimedb::{table, SpacetimeType};

/// Protocol handshake row, singleton `id = 0` (client view `protocol_info`).
#[table(accessor = protocol_info, public)]
pub struct ProtocolInfo {
    #[primary_key]
    pub id: u8,
    pub protocol_version: u32,
    pub min_client_build: u32,
    pub save_format_version: u32,
}

/// Relay/rate-limit configuration, singleton `id = 0` (client view
/// `relay_config`).
#[table(accessor = relay_config, public)]
pub struct RelayConfig {
    #[primary_key]
    pub id: u8,
    pub commands_per_second: u32,
    pub command_rate_window_ms: u32,
    pub max_commit_commands_per_transaction: u32,
    pub default_map_width_tiles: i32,
    pub default_map_height_tiles: i32,
}

/// Audit record kind (plan 01 §6.1); append-only, server-only table.
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuditKind {
    Connect,
    Disconnect,
    ProfileCreate,
    MatchCreate,
    MatchJoin,
    MatchStart,
    ConfigChange,
}
