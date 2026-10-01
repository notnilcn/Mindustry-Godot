// SPDX-License-Identifier: GPL-3.0-only

//! Identity/session/profile/settings tables (plan 01 §6.1). Public accessor
//! names are schema ABI: append-only, never rename.

use spacetimedb::{ConnectionId, Identity, Timestamp, table};

/// One row per identity ever seen; created by `client_connected`, kept across
/// sessions so usernames survive.
#[table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    pub identity: Identity,
    pub username: String,
    pub created_at: Timestamp,
    pub last_seen_at: Timestamp,
    pub protocol_version: u32,
}

/// One row per connection of an identity; `ended_at` closes it.
///
/// `connection_id` is `None` only for server-internal contexts (the reducer
/// context exposes it as an `Option`); client connections always have one.
#[table(accessor = player_session, public, index(accessor = by_identity_started, btree(columns = [identity, started_at])))]
pub struct PlayerSession {
    #[primary_key]
    #[auto_inc]
    pub session_id: u64,
    pub identity: Identity,
    pub connection_id: Option<ConnectionId>,
    pub started_at: Timestamp,
    pub ended_at: Option<Timestamp>,
}

/// Named player profile (plan 12 consumes these; plan 01 stores the skeleton).
#[table(accessor = player_profile, public, index(accessor = by_owner, btree(columns = [identity])))]
pub struct PlayerProfile {
    #[primary_key]
    #[auto_inc]
    pub profile_id: u64,
    pub identity: Identity,
    pub name: String,
    pub created_at: Timestamp,
}

/// Opaque client settings row (plan 14 owns the real settings surface; plan 01
/// validates and stores it).
#[table(accessor = client_settings, public)]
pub struct ClientSettings {
    #[primary_key]
    pub identity: Identity,
    pub ui_scale: f32,
    pub language: String,
    pub music_volume: f32,
    pub sfx_volume: f32,
    pub keybinds_json: String,
    pub revision: u32,
    pub updated_at: Timestamp,
}
