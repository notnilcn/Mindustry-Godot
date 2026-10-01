// SPDX-License-Identifier: GPL-3.0-only

//! P0 tables. The accessor name (`player`) is the generated wave string plan 01 consumes;
//! never rename it (it is part of the client/server schema ABI).

use spacetimedb::{Identity, Timestamp, table};

/// One row per connected / previously seen identity (P0 identity slot).
///
/// Plan 01 extends this into the full profile/session skeleton (`created_at`,
/// `protocol_version`, `player_session`, ...) and moves these reducers into
/// `identity/`. Fields are append-only for the client schema ABI.
#[table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    pub identity: Identity,
    pub username: String,
    pub last_seen: Timestamp,
}
