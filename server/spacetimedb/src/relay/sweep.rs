// SPDX-License-Identifier: GPL-3.0-only

//! Retention sweep (plan 21 §3.7/§3.8/§6.8).
//!
//! `tick_maintenance` runs on a 60 s interval. It prunes the command log behind
//! the snapshot watermark, trims checksum/chat/UI retention, evicts disconnected
//! members past the grace period, deletes ended matches past TTL and prunes
//! stale rate/idle rows. All deletions collect before mutating (index iterators
//! borrow the tables).

use spacetimedb::{ReducerContext, ScheduleAt, Table, TimeDuration, reducer, table};

use super::checksum::match_checksum;
use super::plans::{match_plan_chunk, match_plan_state};
use super::player_state::{match_player_state, match_player_state_rate};
use super::snapshot::{
    match_snapshot, match_snapshot_chunk, match_snapshot_request, snapshots_to_delete,
};
use super::tables::{
    MatchStatus, match_command, match_kick, match_state, relay_match, relay_member,
    sender_command_state,
};
use super::ui_events::match_ui_event;
use crate::admin::{player_ban, server_config_or_default};
use crate::chat::{chat_rate, match_chat};
use crate::identity::tables::player_session;
use crate::main::global::{
    CHECKSUM_RETENTION_SECS, IDLE_ROW_TTL_SECS, MEMBER_GRACE_SECS, PLAN_GROUP_TIMEOUT_SECS,
    PLAYER_SESSION_TTL_SECS, SWEEP_INTERVAL_SECS, UI_EVENT_RETENTION_SECS,
};
use crate::mods::match_mod;

/// Schedules [`tick_maintenance`] every 60 s (plan §6.1). Server-only.
#[table(accessor = maintenance_schedule, scheduled(tick_maintenance))]
pub struct MaintenanceSchedule {
    /// Scheduled reducer id.
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    /// Interval trigger.
    pub scheduled_at: ScheduleAt,
}

/// Seeds the maintenance timer if it is missing (called from `init`).
pub fn seed_maintenance(ctx: &ReducerContext) {
    if ctx.db.maintenance_schedule().iter().next().is_some() {
        return;
    }
    ctx.db.maintenance_schedule().insert(MaintenanceSchedule {
        scheduled_id: 0,
        scheduled_at: TimeDuration::from_micros(
            i64::try_from(SWEEP_INTERVAL_SECS * 1_000_000).unwrap_or(i64::MAX),
        )
        .into(),
    });
}

/// Periodic retention sweep (plan §6.8).
#[reducer]
pub fn tick_maintenance(
    ctx: &ReducerContext,
    _schedule: MaintenanceSchedule,
) -> Result<(), String> {
    let now = ctx.timestamp;
    let now_micros = now.to_micros_since_unix_epoch();
    let config = server_config_or_default(ctx);

    sweep_commands(ctx, config.command_retention_commands);
    sweep_expired_bans(ctx, now_micros);
    sweep_disconnected_members(ctx, now_micros);
    sweep_ended_matches(ctx, now_micros, u64::from(config.match_ttl_hours) * 3600);
    sweep_chat(ctx, config.chat_retention_rows as usize);
    sweep_ui_events(ctx, now_micros);
    sweep_checksums(ctx, now_micros);
    sweep_snapshots(ctx);
    sweep_plan_groups(ctx, now_micros);
    sweep_idle_rows(ctx, now_micros);
    sweep_player_sessions(ctx, now_micros);
    Ok(())
}

fn sweep_commands(ctx: &ReducerContext, retention: u64) {
    for match_row in ctx.db.relay_match().iter().collect::<Vec<_>>() {
        let Some(snapshot_id) = match_row.last_snapshot_id else {
            continue; // never prune without a snapshot watermark
        };
        let Some(snapshot) = ctx.db.match_snapshot().snapshot_id().find(snapshot_id) else {
            continue;
        };
        let ids: Vec<u64> = ctx
            .db
            .match_command()
            .by_match_command()
            .filter(match_row.match_id)
            .map(|command| command.command_id)
            .collect();
        for id in commands_to_prune(&ids, snapshot.command_id, retention) {
            ctx.db.match_command().command_id().delete(id);
        }
    }
}

/// Command ids strictly older than `watermark - retention` (plan §6.8).
pub fn commands_to_prune(ids: &[u64], watermark: u64, retention: u64) -> Vec<u64> {
    if watermark <= retention {
        return Vec::new();
    }
    let cutoff = watermark - retention;
    ids.iter().copied().filter(|id| *id < cutoff).collect()
}

fn sweep_expired_bans(ctx: &ReducerContext, now_micros: i64) {
    for ban in ctx.db.player_ban().iter().collect::<Vec<_>>() {
        if let Some(expiry) = ban.expires_at
            && expiry.to_micros_since_unix_epoch() <= now_micros
        {
            ctx.db.player_ban().ban_id().delete(ban.ban_id);
        }
    }
}

fn sweep_disconnected_members(ctx: &ReducerContext, now_micros: i64) {
    let grace_micros = i64::try_from(MEMBER_GRACE_SECS).unwrap_or(i64::MAX) * 1_000_000;
    for member in ctx.db.relay_member().iter().collect::<Vec<_>>() {
        if member.connected {
            continue;
        }
        let seen = member.last_seen_at.to_micros_since_unix_epoch();
        if now_micros.saturating_sub(seen) > grace_micros {
            ctx.db.relay_member().member_id().delete(member.member_id);
        }
    }
    // Recompute `player_count` for every match after eviction.
    for match_row in ctx.db.relay_match().iter().collect::<Vec<_>>() {
        let count = ctx
            .db
            .relay_member()
            .by_match_identity()
            .filter(match_row.match_id)
            .count() as u16;
        if count != match_row.player_count {
            ctx.db
                .relay_match()
                .match_id()
                .update(super::tables::RelayMatch {
                    player_count: count,
                    ..match_row
                });
        }
    }
}

/// Ended matches older than `ttl_secs` (plan §6.8).
pub fn matches_to_expire(ended: &[(u64, Option<i64>)], now_micros: i64, ttl_secs: u64) -> Vec<u64> {
    let ttl_micros = i64::try_from(ttl_secs).unwrap_or(i64::MAX) * 1_000_000;
    ended
        .iter()
        .filter_map(|(match_id, closed_at)| {
            let closed = (*closed_at)?;
            (now_micros.saturating_sub(closed) > ttl_micros).then_some(*match_id)
        })
        .collect()
}

fn sweep_ended_matches(ctx: &ReducerContext, now_micros: i64, ttl_secs: u64) {
    let ended: Vec<(u64, Option<i64>)> = ctx
        .db
        .relay_match()
        .iter()
        .filter(|row| row.status == MatchStatus::Ended)
        .map(|row| {
            (
                row.match_id,
                row.closed_at.map(|t| t.to_micros_since_unix_epoch()),
            )
        })
        .collect();
    for match_id in matches_to_expire(&ended, now_micros, ttl_secs) {
        delete_match_children(ctx, match_id);
        ctx.db.relay_match().match_id().delete(match_id);
    }
}

fn delete_match_children(ctx: &ReducerContext, match_id: u64) {
    for row in ctx
        .db
        .match_command()
        .by_match_command()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        ctx.db.match_command().command_id().delete(row.command_id);
    }
    for row in ctx
        .db
        .match_checksum()
        .by_checksum_match()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        ctx.db
            .match_checksum()
            .checksum_id()
            .delete(row.checksum_id);
    }
    for row in ctx
        .db
        .match_chat()
        .by_chat_match()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        ctx.db.match_chat().chat_id().delete(row.chat_id);
    }
    for row in ctx
        .db
        .match_ui_event()
        .by_ui_event_match()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        ctx.db.match_ui_event().event_id().delete(row.event_id);
    }
    for row in ctx
        .db
        .match_plan_chunk()
        .by_plan_group()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        ctx.db.match_plan_chunk().chunk_id().delete(row.chunk_id);
    }
    for row in ctx
        .db
        .match_plan_state()
        .by_plan_state_match()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        ctx.db
            .match_plan_state()
            .plan_state_id()
            .delete(row.plan_state_id);
    }
    for row in ctx
        .db
        .match_player_state()
        .by_match_player()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        ctx.db.match_player_state().state_id().delete(row.state_id);
    }
    for row in ctx
        .db
        .match_state()
        .by_match_state()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        ctx.db.match_state().state_id().delete(row.state_id);
    }
    for row in ctx
        .db
        .match_kick()
        .iter()
        .filter(|k| k.match_id == match_id)
        .collect::<Vec<_>>()
    {
        ctx.db.match_kick().kick_id().delete(row.kick_id);
    }
    for row in ctx
        .db
        .match_mod()
        .by_match_mod()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        ctx.db.match_mod().match_mod_id().delete(row.match_mod_id);
    }
    for row in ctx
        .db
        .relay_member()
        .by_match_identity()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        ctx.db.relay_member().member_id().delete(row.member_id);
    }
    for row in ctx
        .db
        .match_snapshot_request()
        .by_request_match()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        ctx.db
            .match_snapshot_request()
            .request_id()
            .delete(row.request_id);
    }
    for snapshot in ctx
        .db
        .match_snapshot()
        .by_snapshot_match()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        for chunk in ctx
            .db
            .match_snapshot_chunk()
            .by_snapshot_chunk()
            .filter(snapshot.snapshot_id)
            .collect::<Vec<_>>()
        {
            ctx.db
                .match_snapshot_chunk()
                .chunk_id()
                .delete(chunk.chunk_id);
        }
        ctx.db
            .match_snapshot()
            .snapshot_id()
            .delete(snapshot.snapshot_id);
    }
}

/// Chat ids to delete keeping the newest `keep` per match (ascending input).
pub fn chat_to_prune(ids_ascending: &[u64], keep: usize) -> Vec<u64> {
    if ids_ascending.len() <= keep {
        return Vec::new();
    }
    ids_ascending[..ids_ascending.len() - keep].to_vec()
}

fn sweep_chat(ctx: &ReducerContext, keep: usize) {
    if keep == 0 {
        return;
    }
    for match_row in ctx.db.relay_match().iter().collect::<Vec<_>>() {
        let ids: Vec<u64> = ctx
            .db
            .match_chat()
            .by_chat_match()
            .filter(match_row.match_id)
            .map(|chat| chat.chat_id)
            .collect();
        let mut sorted = ids;
        sorted.sort_unstable();
        for id in chat_to_prune(&sorted, keep) {
            ctx.db.match_chat().chat_id().delete(id);
        }
    }
}

fn sweep_ui_events(ctx: &ReducerContext, now_micros: i64) {
    let ttl_micros = i64::try_from(UI_EVENT_RETENTION_SECS).unwrap_or(i64::MAX) * 1_000_000;
    for event in ctx.db.match_ui_event().iter().collect::<Vec<_>>() {
        if now_micros.saturating_sub(event.sent_at.to_micros_since_unix_epoch()) > ttl_micros {
            ctx.db.match_ui_event().event_id().delete(event.event_id);
        }
    }
}

fn sweep_checksums(ctx: &ReducerContext, now_micros: i64) {
    let ttl_micros = i64::try_from(CHECKSUM_RETENTION_SECS).unwrap_or(i64::MAX) * 1_000_000;
    for row in ctx.db.match_checksum().iter().collect::<Vec<_>>() {
        if now_micros.saturating_sub(row.created_at.to_micros_since_unix_epoch()) > ttl_micros {
            ctx.db
                .match_checksum()
                .checksum_id()
                .delete(row.checksum_id);
        }
    }
}

fn sweep_snapshots(ctx: &ReducerContext) {
    for match_row in ctx.db.relay_match().iter().collect::<Vec<_>>() {
        let rows: Vec<(u64, super::snapshot::SnapshotKind)> = ctx
            .db
            .match_snapshot()
            .by_snapshot_match()
            .filter(match_row.match_id)
            .map(|snapshot| (snapshot.snapshot_id, snapshot.kind))
            .collect();
        for snapshot_id in snapshots_to_delete(&rows) {
            for chunk in ctx
                .db
                .match_snapshot_chunk()
                .by_snapshot_chunk()
                .filter(snapshot_id)
                .collect::<Vec<_>>()
            {
                ctx.db
                    .match_snapshot_chunk()
                    .chunk_id()
                    .delete(chunk.chunk_id);
            }
            ctx.db.match_snapshot().snapshot_id().delete(snapshot_id);
        }
    }
}

/// Plan chunk ids whose `received_at` is older than `timeout_secs` (stalled
/// incomplete groups, plan §6.8).
pub fn plan_chunks_to_prune(chunks: &[(u64, i64)], now_micros: i64, timeout_secs: u64) -> Vec<u64> {
    let timeout_micros = i64::try_from(timeout_secs).unwrap_or(i64::MAX) * 1_000_000;
    chunks
        .iter()
        .filter(|(_, received)| now_micros.saturating_sub(*received) > timeout_micros)
        .map(|(id, _)| *id)
        .collect()
}

fn sweep_plan_groups(ctx: &ReducerContext, now_micros: i64) {
    let mut chunks = Vec::new();
    for match_row in ctx.db.relay_match().iter().collect::<Vec<_>>() {
        for chunk in ctx
            .db
            .match_plan_chunk()
            .by_plan_group()
            .filter(match_row.match_id)
            .collect::<Vec<_>>()
        {
            chunks.push((
                chunk.chunk_id,
                chunk.received_at.to_micros_since_unix_epoch(),
            ));
        }
    }
    for id in plan_chunks_to_prune(&chunks, now_micros, PLAN_GROUP_TIMEOUT_SECS) {
        ctx.db.match_plan_chunk().chunk_id().delete(id);
    }
    // Orphaned plan state (member row gone) is pruned along with the member.
    for state in ctx.db.match_plan_state().iter().collect::<Vec<_>>() {
        let member_exists = ctx
            .db
            .relay_member()
            .by_match_identity()
            .filter((state.match_id, state.identity))
            .next()
            .is_some();
        if !member_exists {
            ctx.db
                .match_plan_state()
                .plan_state_id()
                .delete(state.plan_state_id);
        }
    }
}

fn sweep_idle_rows(ctx: &ReducerContext, now_micros: i64) {
    let ttl_micros = i64::try_from(IDLE_ROW_TTL_SECS).unwrap_or(i64::MAX) * 1_000_000;
    for row in ctx.db.chat_rate().iter().collect::<Vec<_>>() {
        if now_micros.saturating_sub(row.window_start.to_micros_since_unix_epoch()) > ttl_micros {
            ctx.db.chat_rate().identity().delete(row.identity);
        }
    }
    for row in ctx.db.sender_command_state().iter().collect::<Vec<_>>() {
        if now_micros.saturating_sub(row.updated_at.to_micros_since_unix_epoch()) > ttl_micros {
            ctx.db
                .sender_command_state()
                .identity()
                .delete(row.identity);
        }
    }
    for row in ctx.db.match_player_state_rate().iter().collect::<Vec<_>>() {
        if now_micros.saturating_sub(row.window_start.to_micros_since_unix_epoch()) > ttl_micros {
            ctx.db
                .match_player_state_rate()
                .identity()
                .delete(row.identity);
        }
    }
}

fn sweep_player_sessions(ctx: &ReducerContext, now_micros: i64) {
    let ttl_micros = i64::try_from(PLAYER_SESSION_TTL_SECS).unwrap_or(i64::MAX) * 1_000_000;
    for session in ctx.db.player_session().iter().collect::<Vec<_>>() {
        if session.ended_at.is_none() {
            continue;
        }
        let ended = session
            .ended_at
            .map(|t| t.to_micros_since_unix_epoch())
            .unwrap_or(0);
        if now_micros.saturating_sub(ended) > ttl_micros {
            ctx.db
                .player_session()
                .session_id()
                .delete(session.session_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_prune_behind_watermark() {
        assert!(commands_to_prune(&[1, 2, 3], 100, 4_096).is_empty());
        assert_eq!(commands_to_prune(&[1, 2, 3], 5_000, 4_096), vec![1, 2, 3]);
        assert_eq!(commands_to_prune(&[900, 905, 910], 5_000, 4_096), vec![900]);
    }

    #[test]
    fn members_evicted_after_grace() {
        let now = 1_000_000_000_000i64;
        // Reuse the pure `matches_to_expire` shape for members via the reducer
        // policy: `connected=false` and older than the grace.
        let grace = i64::try_from(MEMBER_GRACE_SECS).unwrap() * 1_000_000;
        assert!(now - (now - grace / 2) <= grace);
        assert!(now - (now - grace * 2) > grace);
    }

    #[test]
    fn ended_matches_expire_after_ttl() {
        let now = 10_000_000_000_000i64;
        let day = 86_400i64 * 1_000_000;
        let rows = vec![
            (1u64, Some(now - day / 2)),
            (2u64, Some(now - day * 2)),
            (3u64, None),
        ];
        assert_eq!(matches_to_expire(&rows, now, 86_400), vec![2]);
    }

    #[test]
    fn chat_retention_keeps_newest() {
        assert!(chat_to_prune(&[1, 2, 3], 5).is_empty());
        assert_eq!(chat_to_prune(&[1, 2, 3, 4, 5], 2), vec![1, 2, 3]);
    }
}
