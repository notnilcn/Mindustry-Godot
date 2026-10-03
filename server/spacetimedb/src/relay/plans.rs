// SPDX-License-Identifier: GPL-3.0-only

//! Chunked LWW plan snapshots (plan 21 §3.6/§6.1; plan 15 §6.5 payload).
//!
//! `report_plan_snapshot` writes one `match_plan_chunk` row per 8 KiB slice of
//! plan 04's `TypeIO.writePlans` bytes. When every chunk of a group is present,
//! `match_plan_state.active_group_id` advances (monotonic by sender) and the
//! previous group's chunks are deleted. Incomplete groups are ignored and swept
//! after `PLAN_GROUP_TIMEOUT`.

use spacetimedb::{Identity, ReducerContext, Table, Timestamp, reducer, table};

use super::methods::{require_match, require_member};
use super::tables::MatchStatus;
use crate::main::global::MAX_PLAN_CHUNK_BYTES;

/// One chunk of a plan-snapshot group (plan §6.1).
#[table(accessor = match_plan_chunk, public, index(accessor = by_plan_group, btree(columns = [match_id, identity, group_id, chunk_index])))]
pub struct MatchPlanChunk {
    /// Auto-inc row id.
    #[primary_key]
    #[auto_inc]
    pub chunk_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Uploading player.
    pub identity: Identity,
    /// Monotonic group id chosen by the sender.
    pub group_id: u32,
    /// Chunk index within the group.
    pub chunk_index: u16,
    /// Total chunks in the group.
    pub chunk_count: u16,
    /// Plan bytes (`TypeIO.writePlans`, ≤ 8 KiB).
    pub plans_blob: Vec<u8>,
    /// Server receive time (incomplete-group sweep).
    pub received_at: Timestamp,
}

/// The active plan group per player (plan §6.1).
#[table(accessor = match_plan_state, public, index(accessor = by_plan_state_match, btree(columns = [match_id, identity])))]
pub struct MatchPlanState {
    /// Auto-inc row id.
    #[primary_key]
    #[auto_inc]
    pub plan_state_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Owning player.
    pub identity: Identity,
    /// Highest fully-uploaded group.
    pub active_group_id: u32,
    /// Last known plan count (`0` while the blob is opaque to the module).
    pub plan_count: u16,
    /// Last update time.
    pub updated_at: Timestamp,
}

/// Appends one plan-snapshot chunk and advances the active group when complete.
#[reducer]
pub fn report_plan_snapshot(
    ctx: &ReducerContext,
    match_id: u64,
    group_id: u32,
    chunk_index: u16,
    chunk_count: u16,
    plans_blob: Vec<u8>,
) -> Result<(), String> {
    let result = report_plan_snapshot_impl(
        ctx,
        match_id,
        group_id,
        chunk_index,
        chunk_count,
        plans_blob,
    );
    if let Err(error) = &result {
        log::warn!("report_plan_snapshot rejected for match {match_id}: {error}");
    }
    result
}

fn report_plan_snapshot_impl(
    ctx: &ReducerContext,
    match_id: u64,
    group_id: u32,
    chunk_index: u16,
    chunk_count: u16,
    plans_blob: Vec<u8>,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    if row.status == MatchStatus::Ended {
        return Err(format!("match {match_id} has ended"));
    }
    let member = require_member(ctx, match_id)?;
    if member.role != super::tables::MemberRole::Player {
        return Err("spectators cannot report plan snapshots".to_string());
    }
    validate_plan_chunk(chunk_index, chunk_count, plans_blob.len())?;

    let identity = ctx.sender();
    let now = ctx.timestamp;
    // Replace a chunk of the same group (idempotent re-upload) rather than
    // inserting duplicates.
    let existing = ctx
        .db
        .match_plan_chunk()
        .by_plan_group()
        .filter((match_id, identity, group_id, chunk_index))
        .next();
    let next = MatchPlanChunk {
        chunk_id: existing.as_ref().map(|chunk| chunk.chunk_id).unwrap_or(0),
        match_id,
        identity,
        group_id,
        chunk_index,
        chunk_count,
        plans_blob,
        received_at: now,
    };
    match existing {
        Some(_) => {
            ctx.db.match_plan_chunk().chunk_id().update(next);
        }
        None => {
            ctx.db.match_plan_chunk().insert(next);
        }
    }

    let present: Vec<u16> = ctx
        .db
        .match_plan_chunk()
        .by_plan_group()
        .filter((match_id, identity, group_id))
        .map(|chunk| chunk.chunk_index)
        .collect();
    if !plan_group_complete(&present, chunk_count) {
        return Ok(());
    }

    let state = ctx
        .db
        .match_plan_state()
        .by_plan_state_match()
        .filter((match_id, identity))
        .next();
    let active = state
        .as_ref()
        .map(|state| state.active_group_id)
        .unwrap_or(0);
    if group_id < active {
        // A late/duplicate group below the active one is discarded.
        return Ok(());
    }

    // Complete: advance the active group and delete the previous group's rows.
    let previous_group = state.as_ref().map(|state| state.active_group_id);
    let next_state = MatchPlanState {
        plan_state_id: state.as_ref().map(|state| state.plan_state_id).unwrap_or(0),
        match_id,
        identity,
        active_group_id: group_id,
        plan_count: 0,
        updated_at: now,
    };
    match state {
        Some(_) => {
            ctx.db.match_plan_state().plan_state_id().update(next_state);
        }
        None => {
            ctx.db.match_plan_state().insert(next_state);
        }
    }
    if let Some(previous) = previous_group
        && previous != group_id
    {
        for chunk in ctx
            .db
            .match_plan_chunk()
            .by_plan_group()
            .filter((match_id, identity, previous))
            .collect::<Vec<_>>()
        {
            ctx.db.match_plan_chunk().chunk_id().delete(chunk.chunk_id);
        }
    }
    Ok(())
}

/// Caps for one plan chunk (plan §6.3/§6.6).
pub fn validate_plan_chunk(
    chunk_index: u16,
    chunk_count: u16,
    blob_len: usize,
) -> Result<(), String> {
    if chunk_count == 0 {
        return Err("chunk_count must be >= 1".to_string());
    }
    if chunk_index >= chunk_count {
        return Err(format!(
            "chunk_index {chunk_index} >= chunk_count {chunk_count}"
        ));
    }
    if blob_len > MAX_PLAN_CHUNK_BYTES {
        return Err(format!("plan chunk exceeds {MAX_PLAN_CHUNK_BYTES} bytes"));
    }
    Ok(())
}

/// Whether every index `0..chunk_count` is present (order-independent, dups ok).
pub fn plan_group_complete(present_indices: &[u16], chunk_count: u16) -> bool {
    if chunk_count == 0 {
        return false;
    }
    (0..chunk_count).all(|index| present_indices.contains(&index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_cap_and_index_bounds() {
        assert!(validate_plan_chunk(0, 1, 10).is_ok());
        assert!(validate_plan_chunk(0, 0, 0).is_err());
        assert!(validate_plan_chunk(3, 3, 0).is_err());
        assert!(validate_plan_chunk(0, 2, MAX_PLAN_CHUNK_BYTES).is_ok());
        assert!(validate_plan_chunk(0, 2, MAX_PLAN_CHUNK_BYTES + 1).is_err());
    }

    #[test]
    fn chunk_reassembly_completeness() {
        assert!(plan_group_complete(&[0, 1, 2], 3));
        assert!(plan_group_complete(&[2, 0, 1], 3));
        assert!(!plan_group_complete(&[0, 1], 3));
        assert!(!plan_group_complete(&[], 1));
        assert!(!plan_group_complete(&[0], 0));
    }

    #[test]
    fn incomplete_group_ignored() {
        // Uploading chunks 0 and 2 of 3 never completes the group.
        assert!(!plan_group_complete(&[0, 2], 3));
    }
}
