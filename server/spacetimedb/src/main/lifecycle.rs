// SPDX-License-Identifier: GPL-3.0-only

//! Lifecycle reducers. Plan 01 extends `init` with real seeds (`protocol_info`,
//! `relay_config`) and adds `player_session` bookkeeping to connect/disconnect.

use spacetimedb::{ReducerContext, Table, reducer};

// 2.10.1 generates the `player` accessor trait; it must be in scope for `ctx.db.player()`.
use super::tables::{Player, player};

/// Seed marker. P0 has no seed tables yet; plan 01 seeds `protocol_info` and
/// `relay_config` here (seeds re-run on every publish, which wipes dev data).
#[reducer(init)]
pub fn init(_ctx: &ReducerContext) {
    log::info!("mindustry_godot P0 skeleton initialized");
}

/// Creates the `player` row on first connect and refreshes `last_seen` afterwards.
#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    let identity = ctx.sender();
    match ctx.db.player().identity().find(identity) {
        Some(player) => {
            ctx.db.player().identity().update(Player {
                last_seen: ctx.timestamp,
                ..player
            });
        }
        None => {
            ctx.db.player().insert(Player {
                identity,
                username: String::new(),
                last_seen: ctx.timestamp,
            });
            log::info!("new player connected: {identity}");
        }
    }
}

/// Stamps `last_seen` for the disconnecting identity (row is never deleted so the
/// identity keeps its username across sessions).
#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    if let Some(player) = ctx.db.player().identity().find(ctx.sender()) {
        ctx.db.player().identity().update(Player {
            last_seen: ctx.timestamp,
            ..player
        });
    }
}
