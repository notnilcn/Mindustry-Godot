// SPDX-License-Identifier: GPL-3.0-only

//! P0 reducers. Plan 01 adds the rest of the identity/session/profile reducers
//! (`create_profile`, `update_client_settings`, ...) and the relay reducers.

use spacetimedb::{ReducerContext, reducer};

// 2.10.1 generates the `player` accessor trait; it must be in scope for `ctx.db.player()`.
use super::tables::{Player, player};

/// Mirrors `Vars.maxNameLength` (`core/src/mindustry/Vars.java:113`).
pub const MAX_USERNAME_LEN: usize = 40;

/// Sets the caller's username.
///
/// `ctx.sender()` is the only principal; the row is created by `client_connected`
/// before any client reducer can run. Usernames are trimmed-empty-checked and
/// length-checked against [`MAX_USERNAME_LEN`].
#[reducer]
pub fn set_username(ctx: &ReducerContext, username: String) -> Result<(), String> {
    let len = username.chars().count();
    if username.trim().is_empty() || len > MAX_USERNAME_LEN {
        return Err(format!(
            "username must be 1-{MAX_USERNAME_LEN} non-whitespace characters"
        ));
    }
    match ctx.db.player().identity().find(ctx.sender()) {
        Some(player) => {
            ctx.db.player().identity().update(Player { username, ..player });
            Ok(())
        }
        // `client_connected` always creates the row first; this guards internal calls.
        None => Err("no player row for caller".to_string()),
    }
}
