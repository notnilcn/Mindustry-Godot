// SPDX-License-Identifier: GPL-3.0-only

//! Per-caller relay views (plan 01 §3.8; plan 21 §6.2). `match_command` and
//! `sender_command_state` are private; these views are the only exposure, and
//! every member-scoped view is a semijoin so nobody can read another match's
//! commands/chat/state.

use spacetimedb::{ViewContext, view};

// 2.10.1 view contexts use read-only accessor traits.
use super::checksum::{MatchChecksum, match_checksum__view};
use super::plans::{
    MatchPlanChunk, MatchPlanState, match_plan_chunk__view, match_plan_state__view,
};
use super::player_state::{MatchPlayerState, match_player_state__view};
use super::snapshot::{
    MatchSnapshot, MatchSnapshotChunk, MatchSnapshotRequest, match_snapshot__view,
    match_snapshot_chunk__view, match_snapshot_request__view,
};
use super::tables::{
    MatchCommand, MatchKick, MatchState, MatchStatus, RelayMatch, RelayMember, SenderCommandState,
    Visibility, match_command__view, match_kick__view, match_state__view, relay_match__view,
    relay_member__view, sender_command_state__view,
};
use super::ui_events::{MatchUiEvent, match_ui_event__view};
use crate::chat::{ChatKind, MatchChat, match_chat__view};

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
    ctx.db.sender_command_state().identity().find(ctx.sender())
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

/// The caller's active match id (highest non-ended match they belong to).
fn active_match_id(ctx: &ViewContext) -> Option<u64> {
    ctx.db
        .relay_member()
        .by_identity()
        .filter(ctx.sender())
        .filter_map(|member| ctx.db.relay_match().match_id().find(member.match_id))
        .filter(|row| row.status != MatchStatus::Ended)
        .max_by_key(|row| row.match_id)
        .map(|row| row.match_id)
}

/// Every member's LWW player state in the caller's active match (plan §6.2).
#[view(accessor = my_match_player_states, public)]
fn my_match_player_states(ctx: &ViewContext) -> Vec<MatchPlayerState> {
    let Some(match_id) = active_match_id(ctx) else {
        return Vec::new();
    };
    ctx.db
        .match_player_state()
        .by_match_player()
        .filter(match_id)
        .collect()
}

/// Active plan state per member in the caller's match (plan §6.2).
#[view(accessor = my_match_plans, public)]
fn my_match_plans(ctx: &ViewContext) -> Vec<MatchPlanState> {
    let Some(match_id) = active_match_id(ctx) else {
        return Vec::new();
    };
    ctx.db
        .match_plan_state()
        .by_plan_state_match()
        .filter(match_id)
        .collect()
}

/// Plan chunks in the caller's match (plan §6.2).
#[view(accessor = my_match_plan_chunks, public)]
fn my_match_plan_chunks(ctx: &ViewContext) -> Vec<MatchPlanChunk> {
    let Some(match_id) = active_match_id(ctx) else {
        return Vec::new();
    };
    ctx.db
        .match_plan_chunk()
        .by_plan_group()
        .filter(match_id)
        .collect()
}

/// UI events for the caller (broadcast or directly targeted; plan §6.2).
#[view(accessor = my_match_ui_events, public)]
fn my_match_ui_events(ctx: &ViewContext) -> Vec<MatchUiEvent> {
    let Some(match_id) = active_match_id(ctx) else {
        return Vec::new();
    };
    ctx.db
        .match_ui_event()
        .by_ui_event_match()
        .filter(match_id)
        .filter(|event| event.target.is_none() || event.target == Some(ctx.sender()))
        .collect()
}

/// Chat visible to the caller: `All` + `System` + own `Team` (plan §6.2).
#[view(accessor = my_match_chat, public)]
fn my_match_chat(ctx: &ViewContext) -> Vec<MatchChat> {
    let Some(member) = active_member(ctx) else {
        return Vec::new();
    };
    ctx.db
        .match_chat()
        .by_chat_match()
        .filter(member.match_id)
        .filter(|chat| match chat.kind {
            ChatKind::All | ChatKind::System => true,
            ChatKind::Team => chat.team.is_some() && chat.team == member.team,
        })
        .collect()
}

/// Checksum rows for the caller's match, bounded by retention (plan §6.2).
#[view(accessor = my_match_checksums, public)]
fn my_match_checksums(ctx: &ViewContext) -> Vec<MatchChecksum> {
    let Some(match_id) = active_match_id(ctx) else {
        return Vec::new();
    };
    ctx.db
        .match_checksum()
        .by_checksum_match()
        .filter(match_id)
        .collect()
}

/// Snapshot metadata for the caller's match (no chunks; plan §6.2).
#[view(accessor = my_match_snapshots, public)]
fn my_match_snapshots(ctx: &ViewContext) -> Vec<MatchSnapshot> {
    let Some(match_id) = active_match_id(ctx) else {
        return Vec::new();
    };
    ctx.db
        .match_snapshot()
        .by_snapshot_match()
        .filter(match_id)
        .collect()
}

/// Snapshot chunks for the caller's match (on-demand Snapshot wave; plan §6.2).
#[view(accessor = my_match_snapshot_chunks, public)]
fn my_match_snapshot_chunks(ctx: &ViewContext) -> Vec<MatchSnapshotChunk> {
    let Some(match_id) = active_match_id(ctx) else {
        return Vec::new();
    };
    ctx.db
        .match_snapshot()
        .by_snapshot_match()
        .filter(match_id)
        .flat_map(|snapshot| {
            ctx.db
                .match_snapshot_chunk()
                .by_snapshot_chunk()
                .filter(snapshot.snapshot_id)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Pending snapshot requests, visible to the host only (plan §6.2).
#[view(accessor = my_match_snapshot_requests, public)]
fn my_match_snapshot_requests(ctx: &ViewContext) -> Vec<MatchSnapshotRequest> {
    let Some(match_id) = active_match_id(ctx) else {
        return Vec::new();
    };
    let Some(row) = ctx.db.relay_match().match_id().find(match_id) else {
        return Vec::new();
    };
    if row.host != ctx.sender() {
        return Vec::new();
    }
    ctx.db
        .match_snapshot_request()
        .by_request_match()
        .filter(match_id)
        .collect()
}

/// The caller's active member row (for team-scoped views).
fn active_member(ctx: &ViewContext) -> Option<RelayMember> {
    let match_id = active_match_id(ctx)?;
    ctx.db
        .relay_member()
        .by_match_identity()
        .filter((match_id, ctx.sender()))
        .next()
}
