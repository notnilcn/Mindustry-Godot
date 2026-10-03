// SPDX-License-Identifier: GPL-3.0-only

//! Per-caller relay views (plan 01 §3.8; plan 21 §6.2). `match_command` and
//! `sender_command_state` are private; these views are the only exposure, and
//! every member-scoped view is a semijoin so nobody can read another match's
//! commands/chat/state.

use spacetimedb::{ViewContext, view};

// 2.10.1 view contexts use read-only accessor traits.
use super::tables::{
    MatchCommand, MatchKick, MatchState, MatchStatus, RelayMatch, RelayMember, SenderCommandState,
    Visibility, match_command__view, match_kick__view, match_state__view, relay_match__view,
    relay_member__view, sender_command_state__view,
};

/// Every public, non-ended match for the server browser (plan §3.11).
#[view(accessor = all_matches, public)]
fn all_matches(ctx: &ViewContext) -> Vec<RelayMatch> {
    ctx.db
        .relay_match()
        .by_visibility()
        .filter(Visibility::Public)
        .filter(|row| row.status != MatchStatus::Ended)
        .collect()
}

/// Every non-ended match the caller belongs to.
#[view(accessor = my_matches, public)]
fn my_matches(ctx: &ViewContext) -> Vec<RelayMatch> {
    ctx.db
        .relay_member()
        .by_identity()
        .filter(ctx.sender())
        .filter_map(|member| ctx.db.relay_match().match_id().find(member.match_id))
        .filter(|row| row.status != MatchStatus::Ended)
        .collect()
}

/// The caller's most recently created active match (highest `match_id`).
#[view(accessor = my_match, public)]
fn my_match(ctx: &ViewContext) -> Option<RelayMatch> {
    ctx.db
        .relay_member()
        .by_identity()
        .filter(ctx.sender())
        .filter_map(|member| ctx.db.relay_match().match_id().find(member.match_id))
        .filter(|row| row.status != MatchStatus::Ended)
        .max_by_key(|row| row.match_id)
}

/// Members of the caller's active match.
#[view(accessor = my_match_members, public)]
fn my_match_members(ctx: &ViewContext) -> Vec<RelayMember> {
    ctx.db
        .relay_member()
        .by_identity()
        .filter(ctx.sender())
        .flat_map(|member| {
            ctx.db
                .relay_member()
                .by_match_identity()
                .filter(member.match_id)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The latest host-published state for the caller's active match.
#[view(accessor = my_match_state, public)]
fn my_match_state(ctx: &ViewContext) -> Option<MatchState> {
    let active = ctx
        .db
        .relay_member()
        .by_identity()
        .filter(ctx.sender())
        .filter_map(|member| ctx.db.relay_match().match_id().find(member.match_id))
        .filter(|row| row.status != MatchStatus::Ended)
        .max_by_key(|row| row.match_id)?;
    ctx.db
        .match_state()
        .by_match_state()
        .filter(active.match_id)
        .max_by_key(|state| state.updated_at.to_micros_since_unix_epoch())
}

/// Kick records targeting the caller (drives the disconnect UI).
#[view(accessor = my_kick, public)]
fn my_kick(ctx: &ViewContext) -> Vec<MatchKick> {
    ctx.db
        .match_kick()
        .by_kick_target()
        .filter(ctx.sender())
        .collect()
}

/// The caller's own per-sender command bookkeeping (echo/rejection).
#[view(accessor = my_sender_command_state, public)]
fn my_sender_command_state(ctx: &ViewContext) -> Option<SenderCommandState> {
    ctx.db
        .sender_command_state()
        .identity()
        .find(ctx.sender())
}

/// The caller's command log across their matches, in `command_id` order
/// per match (global ordering is the client's job; R13).
#[view(accessor = my_match_commands, public)]
fn my_match_commands(ctx: &ViewContext) -> Vec<MatchCommand> {
    ctx.db
        .relay_member()
        .by_identity()
        .filter(ctx.sender())
        .flat_map(|member| {
            ctx.db
                .match_command()
                .by_match_command()
                .filter(member.match_id)
                .collect::<Vec<_>>()
        })
        .collect()
}
