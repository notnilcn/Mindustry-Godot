// SPDX-License-Identifier: GPL-3.0-only

//! Lifecycle reducers (plan 01 §3.7): `init` seeds the singletons,
//! `client_connected` upserts the player and opens a session,
//! `client_disconnected` stamps `last_seen_at` and closes the open session(s).

use spacetimedb::{ReducerContext, Table, reducer};

use super::audit::audit;
use super::global::PROTOCOL_VERSION;
use super::seeds::{
    seed_admin_identities, seed_protocol_info, seed_relay_config, seed_server_config,
};
use super::tables::AuditKind;
use crate::identity::tables::{
    Player, PlayerSession, player, player_session,
};

/// Seeds protocol/config singletons on publish. Publishing wipes dev data, so
/// seeds must be self-contained (main/server rule).
#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    seed_protocol_info(ctx);
    seed_relay_config(ctx);
    seed_server_config(ctx);
    seed_admin_identities(ctx);
    audit(
        ctx,
        None,
        AuditKind::ConfigChange,
        "init: seeded protocol_info and relay_config",
    );
    log::info!("mindustry_godot initialized (protocol {PROTOCOL_VERSION})");
}

/// Creates the `player` row on first connect, refreshes `last_seen_at`
/// afterwards, and opens a `player_session` row.
#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    let identity = ctx.sender();
    let now = ctx.timestamp;
    match ctx.db.player().identity().find(identity) {
        Some(player_row) => {
            ctx.db.player().identity().update(Player {
                last_seen_at: now,
                protocol_version: PROTOCOL_VERSION,
                ..player_row
            });
        }
        None => {
            ctx.db.player().insert(Player {
                identity,
                username: String::new(),
                created_at: now,
                last_seen_at: now,
                protocol_version: PROTOCOL_VERSION,
            });
            log::info!("new player connected: {identity}");
        }
    }
    ctx.db.player_session().insert(PlayerSession {
        session_id: 0,
        identity,
        connection_id: ctx.connection_id(),
        started_at: now,
        ended_at: None,
    });
    audit(ctx, Some(identity), AuditKind::Connect, "client connected");
}

/// Stamps `last_seen_at` and closes every still-open session for the caller.
/// Player rows are never deleted so usernames survive across sessions.
#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    let identity = ctx.sender();
    if let Some(player_row) = ctx.db.player().identity().find(identity) {
        ctx.db.player().identity().update(Player {
            last_seen_at: ctx.timestamp,
            ..player_row
        });
    }
    // Collect before mutating: the index iterator borrows the table.
    for session in ctx
        .db
        .player_session()
        .by_identity_started()
        .filter(identity)
        .collect::<Vec<_>>()
    {
        if session.ended_at.is_none() {
            ctx.db.player_session().session_id().update(PlayerSession {
                ended_at: Some(ctx.timestamp),
                ..session
            });
        }
    }
    audit(
        ctx,
        Some(identity),
        AuditKind::Disconnect,
        "client disconnected",
    );
}
