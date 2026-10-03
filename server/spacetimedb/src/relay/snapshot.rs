// SPDX-License-Identifier: GPL-3.0-only

//! Host-published snapshots and late-join streaming (plan 21 §3.7/§3.8/§6.1).
//!
//! The host publishes three kinds of row: `WorldReset` (command 0), `Dynamic`
//! (a chunked, compressed plan-04 entity/team blob) and `Digest` (checksum/rng
//! only). A member requests a fresh `Dynamic` snapshot with `request_snapshot`;
//! the host watches `my_match_snapshot_requests` and publishes. Retention keeps
//! the newest of each kind (plan §6.8).

use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, Timestamp, reducer, table};

use super::methods::{is_host_or_admin, require_match, require_member, window_expired};
use super::tables::{MatchStatus, relay_match};
use crate::main::global::{
    MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_CHUNKS, SNAPSHOT_CHUNK_BYTES, SNAPSHOT_RATE_MAX,
    SNAPSHOT_RATE_WINDOW_MS,
};

/// Snapshot row kind (plan §3.8; variant names are ABI).
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum SnapshotKind {
    /// `command_id = 0` world reset (rules/map identity, empty blob).
    WorldReset,
    /// Full dynamic state blob.
    Dynamic,
    /// Checksum/digest only (no blob).
    Digest,
}

/// Snapshot metadata (plan §6.1). Chunks live in `match_snapshot_chunk`.
#[table(accessor = match_snapshot, public, index(accessor = by_snapshot_match, btree(columns = [match_id, created_at])))]
pub struct MatchSnapshot {
    /// Auto-inc snapshot id.
    #[primary_key]
    #[auto_inc]
    pub snapshot_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Publishing host.
    pub author: Identity,
    /// Snapshot kind.
    pub kind: SnapshotKind,
    /// Blob format version.
    pub format: u32,
    /// Plan 05 `CHECKSUM_VERSION`.
    pub checksum_version: u32,
    /// Command watermark included in the blob.
    pub command_id: u64,
    /// Diagnostic sim tick.
    pub sim_tick: u64,
    /// Scoped checksum at the watermark.
    pub checksum: u64,
    /// Map identity.
    pub map_id: String,
    /// Map seed.
    pub map_seed: u64,
    /// Map generator hash.
    pub map_hash: u64,
    /// Client build id.
    pub build_id: String,
    /// Content manifest hash.
    pub content_hash: u64,
    /// Uncompressed blob length in bytes.
    pub bytes_len: u32,
    /// Number of `match_snapshot_chunk` rows.
    pub chunk_count: u16,
    /// Publish time.
    pub created_at: Timestamp,
}

/// One snapshot blob chunk (plan §6.1; server-only, view-exposed).
#[table(accessor = match_snapshot_chunk, index(accessor = by_snapshot_chunk, btree(columns = [snapshot_id, chunk_index])))]
pub struct MatchSnapshotChunk {
    /// Auto-inc row id.
    #[primary_key]
    #[auto_inc]
    pub chunk_id: u64,
    /// Owning snapshot.
    #[index(btree)]
    pub snapshot_id: u64,
    /// Chunk index (`0..chunk_count`).
    pub chunk_index: u16,
    /// Compression-wrapped chunk bytes (≤ 16 KiB).
    pub data: Vec<u8>,
}

/// Pending snapshot requests (plan §6.1; server-only, host view).
#[table(accessor = match_snapshot_request, index(accessor = by_request_match, btree(columns = [match_id])))]
pub struct MatchSnapshotRequest {
    /// Auto-inc request id.
    #[primary_key]
    #[auto_inc]
    pub request_id: u64,
    /// Owning match.
    pub match_id: u64,
    /// Requesting member.
    pub identity: Identity,
    /// Request time.
    pub requested_at: Timestamp,
}

/// Server-only snapshot request rate gate (plan §3.7: 1/10 s).
#[table(accessor = snapshot_rate)]
pub struct SnapshotRate {
    /// Caller.
    #[primary_key]
    pub identity: Identity,
    /// Current window start.
    pub window_start: Timestamp,
    /// Requests in the window.
    pub count: u32,
}

/// Requests a fresh host snapshot (member; rate 1/10 s).
#[reducer]
pub fn request_snapshot(ctx: &ReducerContext, match_id: u64) -> Result<(), String> {
    let result = request_snapshot_impl(ctx, match_id);
    if let Err(error) = &result {
        log::warn!("request_snapshot rejected for match {match_id}: {error}");
    }
    result
}

fn request_snapshot_impl(ctx: &ReducerContext, match_id: u64) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    if row.status == MatchStatus::Ended {
        return Err(format!("match {match_id} has ended"));
    }
    require_member(ctx, match_id)?;
    rate_limit_snapshot(ctx)?;
    ctx.db
        .match_snapshot_request()
        .insert(MatchSnapshotRequest {
            request_id: 0,
            match_id,
            identity: ctx.sender(),
            requested_at: ctx.timestamp,
        });
    Ok(())
}

/// Publishes a snapshot (host/admin only; splits `blob` into chunks).
#[reducer]
#[allow(clippy::too_many_arguments)]
pub fn publish_snapshot(
    ctx: &ReducerContext,
    match_id: u64,
    kind: SnapshotKind,
    format: u32,
    checksum_version: u32,
    command_id: u64,
    sim_tick: u64,
    checksum: u64,
    map_id: String,
    map_seed: u64,
    map_hash: u64,
    build_id: String,
    content_hash: u64,
    blob: Vec<u8>,
) -> Result<(), String> {
    let result = publish_snapshot_impl(
        ctx,
        match_id,
        kind,
        format,
        checksum_version,
        command_id,
        sim_tick,
        checksum,
        map_id,
        map_seed,
        map_hash,
        build_id,
        content_hash,
        blob,
    );
    if let Err(error) = &result {
        log::warn!("publish_snapshot rejected for match {match_id}: {error}");
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn publish_snapshot_impl(
    ctx: &ReducerContext,
    match_id: u64,
    kind: SnapshotKind,
    format: u32,
    checksum_version: u32,
    command_id: u64,
    sim_tick: u64,
    checksum: u64,
    map_id: String,
    map_seed: u64,
    map_hash: u64,
    build_id: String,
    content_hash: u64,
    blob: Vec<u8>,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    if row.status == MatchStatus::Ended {
        return Err(format!("match {match_id} has ended"));
    }
    require_member(ctx, match_id)?;
    if !is_host_or_admin(ctx, &row) {
        return Err("only the host or an admin may publish snapshots".to_string());
    }
    if kind == SnapshotKind::Digest && !blob.is_empty() {
        return Err("digest snapshots carry no blob".to_string());
    }
    let chunks = validate_snapshot_blob(&blob)?;

    let snapshot = ctx.db.match_snapshot().insert(MatchSnapshot {
        snapshot_id: 0,
        match_id,
        author: ctx.sender(),
        kind,
        format,
        checksum_version,
        command_id,
        sim_tick,
        checksum,
        map_id,
        map_seed,
        map_hash,
        build_id,
        content_hash,
        bytes_len: blob.len() as u32,
        chunk_count: chunks as u16,
        created_at: ctx.timestamp,
    });
    for (index, data) in blob.chunks(SNAPSHOT_CHUNK_BYTES as usize).enumerate() {
        ctx.db.match_snapshot_chunk().insert(MatchSnapshotChunk {
            chunk_id: 0,
            snapshot_id: snapshot.snapshot_id,
            chunk_index: index as u16,
            data: data.to_vec(),
        });
    }
    if kind != SnapshotKind::Digest {
        ctx.db
            .relay_match()
            .match_id()
            .update(super::tables::RelayMatch {
                last_snapshot_id: Some(snapshot.snapshot_id),
                ..row
            });
    }
    prune_snapshots(ctx, match_id);
    Ok(())
}

fn rate_limit_snapshot(ctx: &ReducerContext) -> Result<(), String> {
    let identity = ctx.sender();
    let now = ctx.timestamp;
    let now_micros = now.to_micros_since_unix_epoch();
    let existing = ctx.db.snapshot_rate().identity().find(identity);
    let mut rate = existing.unwrap_or(SnapshotRate {
        identity,
        window_start: now,
        count: 0,
    });
    if window_expired(
        now_micros,
        rate.window_start.to_micros_since_unix_epoch(),
        SNAPSHOT_RATE_WINDOW_MS,
    ) {
        rate.window_start = now;
        rate.count = 0;
    }
    if rate.count >= SNAPSHOT_RATE_MAX {
        return Err(format!(
            "snapshot request rate exceeded ({SNAPSHOT_RATE_MAX}/{SNAPSHOT_RATE_WINDOW_MS} ms)"
        ));
    }
    rate.count += 1;
    if ctx.db.snapshot_rate().identity().find(identity).is_some() {
        ctx.db.snapshot_rate().identity().update(rate);
    } else {
        ctx.db.snapshot_rate().insert(rate);
    }
    Ok(())
}

/// Caps a blob to [`MAX_SNAPSHOT_BYTES`]/[`MAX_SNAPSHOT_CHUNKS`]; returns the
/// chunk count.
pub fn validate_snapshot_blob(blob: &[u8]) -> Result<usize, String> {
    if blob.len() > MAX_SNAPSHOT_BYTES {
        return Err(format!("snapshot exceeds {MAX_SNAPSHOT_BYTES} bytes"));
    }
    let chunks = blob.len().div_ceil(SNAPSHOT_CHUNK_BYTES as usize);
    if chunks > MAX_SNAPSHOT_CHUNKS {
        return Err(format!("snapshot exceeds {MAX_SNAPSHOT_CHUNKS} chunks"));
    }
    Ok(chunks)
}

/// Snapshot ids to delete from `(snapshot_id, kind)` rows: keep the newest row
/// of each kind (plan §6.8 "keep-3").
pub fn snapshots_to_delete(rows: &[(u64, SnapshotKind)]) -> Vec<u64> {
    let mut newest: [Option<u64>; 3] = [None, None, None];
    for (id, kind) in rows {
        let slot = kind_slot(*kind);
        if newest[slot].map(|current| *id > current).unwrap_or(true) {
            newest[slot] = Some(*id);
        }
    }
    rows.iter()
        .filter(|(id, kind)| newest[kind_slot(*kind)] != Some(*id))
        .map(|(id, _)| *id)
        .collect()
}

fn kind_slot(kind: SnapshotKind) -> usize {
    match kind {
        SnapshotKind::WorldReset => 0,
        SnapshotKind::Dynamic => 1,
        SnapshotKind::Digest => 2,
    }
}

fn prune_snapshots(ctx: &ReducerContext, match_id: u64) {
    let rows: Vec<(u64, SnapshotKind)> = ctx
        .db
        .match_snapshot()
        .by_snapshot_match()
        .filter(match_id)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_cap_and_chunk_count() {
        assert_eq!(validate_snapshot_blob(&[]).unwrap(), 0);
        assert_eq!(validate_snapshot_blob(&vec![0u8; 10]).unwrap(), 1);
        assert_eq!(
            validate_snapshot_blob(&vec![0u8; SNAPSHOT_CHUNK_BYTES as usize * 2 + 1]).unwrap(),
            3
        );
        assert!(validate_snapshot_blob(&vec![0u8; MAX_SNAPSHOT_BYTES + 1]).is_err());
    }

    #[test]
    fn snapshot_keep_three() {
        let rows = vec![
            (1, SnapshotKind::WorldReset),
            (2, SnapshotKind::Dynamic),
            (3, SnapshotKind::Dynamic),
            (4, SnapshotKind::Digest),
            (5, SnapshotKind::Digest),
        ];
        let mut deleted = snapshots_to_delete(&rows);
        deleted.sort_unstable();
        // Newest per kind (1, 3, 5) survive.
        assert_eq!(deleted, vec![2, 4]);
    }

    #[test]
    fn dynamic_snapshot_roundtrip_placeholder() {
        // The byte-exact dynamic codec is `mind-stdb::snapshot`; the server only
        // stores opaque chunks, so this asserts chunk splitting is lossless.
        let blob: Vec<u8> = (0..(SNAPSHOT_CHUNK_BYTES as usize + 7))
            .map(|i| i as u8)
            .collect();
        let chunks: Vec<Vec<u8>> = blob
            .chunks(SNAPSHOT_CHUNK_BYTES as usize)
            .map(<[u8]>::to_vec)
            .collect();
        let reassembled: Vec<u8> = chunks.concat();
        assert_eq!(reassembled, blob);
    }
}
