// SPDX-License-Identifier: GPL-3.0-only

//! Admin/moderation tables and cheap checks (plan 21 §3.10/§6.1).
//!
//! Plan 21 M1 ships the singleton config, the global admin identity set, the
//! identity ban list and the whitelist so join validation has a data source.
//! The full admin reducer surface (`admin_kick`/`admin_ban`/…) is plan 21 M6.

use spacetimedb::{Identity, ReducerContext, Timestamp, table};

/// Module-wide server configuration, singleton `id = 0` (plan §6.1).
#[table(accessor = server_config, public)]
pub struct ServerConfig {
    #[primary_key]
    pub id: u8,
    pub server_name: String,
    pub description: String,
    pub motd: String,
    pub whitelist_enabled: bool,
    pub allow_custom_clients: bool,
    pub max_players_default: u16,
    pub auto_pause: bool,
    pub chat_rate_window_ms: u32,
    pub chat_rate_max: u32,
    pub snapshot_interval_ticks: u32,
    pub digest_interval_ticks: u32,
    pub checksum_interval_ticks: u32,
    pub command_retention_commands: u64,
    pub chat_retention_rows: u32,
    pub match_ttl_hours: u32,
    pub banned_name_patterns: Vec<String>,
}

/// Global admin grant (plan §6.1); bootstrap seed in [`super::main::seeds`].
#[table(accessor = admin_identity, public)]
pub struct AdminIdentity {
    #[primary_key]
    pub identity: Identity,
    pub granted_by: Identity,
    pub granted_at: Timestamp,
}

/// Identity ban (plan §3.10; no IP/subnet bans under D2).
#[table(accessor = player_ban, public, index(accessor = by_ban_identity, btree(columns = [identity])))]
pub struct PlayerBan {
    #[primary_key]
    #[auto_inc]
    pub ban_id: u64,
    pub identity: Identity,
    pub reason: String,
    pub banned_by: Identity,
    pub created_at: Timestamp,
    pub expires_at: Option<Timestamp>,
}

/// Whitelist entry (plan §3.10/§6.1).
#[table(accessor = whitelist_entry, public)]
pub struct WhitelistEntry {
    #[primary_key]
    pub identity: Identity,
    pub added_by: Identity,
    pub added_at: Timestamp,
}

/// Defaults for the [`ServerConfig`] singleton (plan §6.8).
pub fn default_server_config() -> ServerConfig {
    ServerConfig {
        id: 0,
        server_name: "Mindustry-Godot".to_string(),
        description: String::new(),
        motd: String::new(),
        whitelist_enabled: false,
        allow_custom_clients: true,
        max_players_default: crate::main::global::DEFAULT_MAX_PLAYERS,
        auto_pause: false,
        chat_rate_window_ms: 2_000,
        chat_rate_max: 20,
        snapshot_interval_ticks: crate::main::global::SNAPSHOT_INTERVAL_TICKS as u32,
        digest_interval_ticks: crate::main::global::DIGEST_INTERVAL_TICKS as u32,
        checksum_interval_ticks: crate::main::global::CHECKSUM_INTERVAL_TICKS as u32,
        command_retention_commands: crate::main::global::REPLAY_TAIL,
        chat_retention_rows: 500,
        match_ttl_hours: 24,
        banned_name_patterns: Vec::new(),
    }
}

/// Loads the singleton config (defaults when `init` has not seeded yet).
pub fn server_config_or_default(ctx: &ReducerContext) -> ServerConfig {
    ctx.db
        .server_config()
        .id()
        .find(0u8)
        .unwrap_or_else(default_server_config)
}
