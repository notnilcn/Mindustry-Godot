// SPDX-License-Identifier: GPL-3.0-only

//! Per-caller relay views (plan 01 §3.8). `match_command` is private; these
//! views are the only exposure, and `my_match_commands` is a member semijoin
//! so nobody can subscribe to another match's log.

use spacetimedb::{ViewContext, view};

// 2.10.1 view contexts use read-only accessor traits.
use super::tables::{
    MatchCommand, MatchStatus, RelayMatch, match_command__view, relay_match__view,
    relay_member__view,
};

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
