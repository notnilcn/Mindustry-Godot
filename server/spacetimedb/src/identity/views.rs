// SPDX-License-Identifier: GPL-3.0-only

//! Per-caller identity views (plan 01 §3.6/§6.1). These are the Base/Lobby wave
//! members: clients subscribe to the accessor names, never to raw private data.

use spacetimedb::{AnonymousViewContext, Query, ViewContext, view};

// 2.10.1 view contexts use read-only accessor traits: `<accessor>__view`; the
// query-builder accessor trait is `<accessor>__query`.
use super::tables::{
    ClientSettings, Player, PlayerProfile, client_settings__view, player__query, player__view,
    player_profile__view,
};

/// The caller's own `player` row (none before `client_connected`).
#[view(accessor = local_player, public)]
fn local_player(ctx: &ViewContext) -> Option<Player> {
    ctx.db.player().identity().find(ctx.sender())
}

/// The caller's most recently created profile (highest auto-inc `profile_id`).
#[view(accessor = local_player_profile, public)]
fn local_player_profile(ctx: &ViewContext) -> Option<PlayerProfile> {
    ctx.db.player_profile().by_owner().filter(ctx.sender()).last()
}

/// Every player row; plan 21 adds AOI-scoped projections when the roster grows.
///
/// Query-style view (`impl Query`) because read-only view handles expose only
/// `count()` + index accessors, not a full-table iterator.
#[view(accessor = all_players, public)]
fn all_players(ctx: &AnonymousViewContext) -> impl Query<Player> {
    ctx.from.player().build()
}

/// The caller's own client settings row.
#[view(accessor = local_client_settings, public)]
fn local_client_settings(ctx: &ViewContext) -> Option<ClientSettings> {
    ctx.db.client_settings().identity().find(ctx.sender())
}
