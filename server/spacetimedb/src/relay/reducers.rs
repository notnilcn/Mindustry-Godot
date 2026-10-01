// SPDX-License-Identifier: GPL-3.0-only

//! Relay reducers (plan 01 §3.8/§3.9). Every reducer is deterministic, returns
//! no data (`Result<(), String>` at most), uses `ctx.sender()` as the only
//! principal and performs cheap validation only (D2).

use spacetimedb::{ReducerContext, Table, reducer};

use super::methods::{
    MAX_MAP_ID_LEN, rate_allow, relay_config_or_default, require_match, require_member,
    require_status, validate_kind,
};
use super::tables::{
    AuthorityMode, CommandKind, MatchCommand, MatchStatus, RelayMatch, RelayMember, match_command,
    relay_match, relay_member,
};
use crate::main::audit::audit;
use crate::main::global::PROTOCOL_VERSION;
use crate::main::tables::AuditKind;

/// Creates a lobby match and auto-joins the creator.
#[reducer]
pub fn create_match(ctx: &ReducerContext, map_id: String, map_seed: u64) -> Result<(), String> {
    let map_id = map_id.trim().to_string();
    if map_id.is_empty() || map_id.chars().count() > MAX_MAP_ID_LEN {
        return Err(format!("map_id must be 1-{MAX_MAP_ID_LEN} characters"));
    }
    let config = relay_config_or_default(ctx);
    let now = ctx.timestamp;
    let row = ctx.db.relay_match().insert(RelayMatch {
        match_id: 0,
        map_id,
        map_seed,
        map_width_tiles: config.default_map_width_tiles,
        map_height_tiles: config.default_map_height_tiles,
        status: MatchStatus::Lobby,
        authority: AuthorityMode::Relay,
        protocol_version: PROTOCOL_VERSION,
        created_by: ctx.sender(),
        created_at: now,
        started_at: None,
        ended_at: None,
    });
    ctx.db.relay_member().insert(RelayMember {
        member_id: 0,
        match_id: row.match_id,
        identity: ctx.sender(),
        joined_at: now,
    });
    audit(
        ctx,
        Some(ctx.sender()),
        AuditKind::MatchCreate,
        format!("match {} `{}`", row.match_id, row.map_id),
    );
    Ok(())
}

/// Joins an existing non-ended match (idempotency is an error, not a no-op).
#[reducer]
pub fn join_match(ctx: &ReducerContext, match_id: u64) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    if row.status == MatchStatus::Ended {
        return Err(format!("match {match_id} has ended"));
    }
    if require_member(ctx, match_id).is_ok() {
        return Err("caller is already a member of this match".to_string());
    }
    ctx.db.relay_member().insert(RelayMember {
        member_id: 0,
        match_id,
        identity: ctx.sender(),
        joined_at: ctx.timestamp,
    });
    audit(
        ctx,
        Some(ctx.sender()),
        AuditKind::MatchJoin,
        format!("joined match {match_id}"),
    );
    Ok(())
}

/// Leaves a match; the creator may leave too (plan 21 owns host handover).
#[reducer]
pub fn leave_match(ctx: &ReducerContext, match_id: u64) -> Result<(), String> {
    let member = require_member(ctx, match_id)?;
    ctx.db.relay_member().member_id().delete(member.member_id);
    Ok(())
}

/// Starts a lobby match (any member today; plan 21 adds the host role).
#[reducer]
pub fn start_match(ctx: &ReducerContext, match_id: u64) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    require_status(&row, MatchStatus::Lobby)?;
    require_member(ctx, match_id)?;
    ctx.db.relay_match().match_id().update(RelayMatch {
        status: MatchStatus::Running,
        started_at: Some(ctx.timestamp),
        ..row
    });
    audit(
        ctx,
        Some(ctx.sender()),
        AuditKind::MatchStart,
        format!("started match {match_id}"),
    );
    Ok(())
}

/// Appends one validated command to the private relay log (plan §3.9).
///
/// `sender_seq` is assigned by the server from the per-identity
/// `CommandRate.last_sender_seq` (monotonic across window rolls), so clients
/// cannot replay old sequences; `command_id` is the global commit order.
#[reducer]
pub fn send_match_command(
    ctx: &ReducerContext,
    match_id: u64,
    client_tick: u64,
    kind: CommandKind,
) -> Result<(), String> {
    let result = send_match_command_impl(ctx, match_id, client_tick, kind);
    if let Err(error) = &result {
        // Failed reducers roll back, so rejections log instead of auditing (R7).
        log::warn!("send_match_command rejected for match {match_id}: {error}");
    }
    result
}

fn send_match_command_impl(
    ctx: &ReducerContext,
    match_id: u64,
    client_tick: u64,
    kind: CommandKind,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    require_status(&row, MatchStatus::Running)?;
    require_member(ctx, match_id)?;
    if row.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "match protocol {} != module protocol {PROTOCOL_VERSION}",
            row.protocol_version
        ));
    }
    validate_kind(&kind, row.map_width_tiles, row.map_height_tiles)?;
    let config = relay_config_or_default(ctx);
    let sender_seq = rate_allow(ctx, &config)?;
    ctx.db.match_command().insert(MatchCommand {
        command_id: 0,
        match_id,
        sender: ctx.sender(),
        sender_seq,
        client_tick,
        kind,
        sent_at: ctx.timestamp,
    });
    Ok(())
}
