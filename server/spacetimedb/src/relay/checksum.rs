// SPDX-License-Identifier: GPL-3.0-only

//! Checksum publication and desync detection (plan 21 §3.7/§6.1/§6.5).
//!
//! Every member publishes a scoped checksum keyed on its `command_id`
//! watermark. Comparison groups rows by `command_id`, ignores rows whose
//! `checksum_version` differs (plan 05 requirement) and reports a desync when
//! two version-consistent peers disagree. The host's value is canonical under
//! D2; when the host is the outlier the majority is recorded but correction
//! still targets the host (§3.7).

use std::collections::BTreeMap;

use spacetimedb::{Identity, ReducerContext, Table, Timestamp, reducer, table};

use super::methods::{require_match, require_member, window_expired};
use super::tables::MatchStatus;
use crate::main::global::{
    CHECKSUM_INTERVAL_TICKS, CHECKSUM_RATE_MAX, CHECKSUM_RATE_WINDOW_MS, CHECKSUM_VERSION,
};

/// One peer checksum report (plan §6.1).
#[table(accessor = match_checksum, public, index(accessor = by_checksum_match, btree(columns = [match_id, command_id])))]
pub struct MatchChecksum {
    /// Auto-inc row id.
    #[primary_key]
    #[auto_inc]
    pub checksum_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Reporting peer.
    pub sender: Identity,
    /// Command watermark: state after all commands `<= command_id`.
    pub command_id: u64,
    /// Diagnostic sim tick.
    pub sim_tick: u64,
    /// Scoped checksum value.
    pub checksum: u64,
    /// Plan 05 `CHECKSUM_VERSION`.
    pub checksum_version: u32,
    /// `ChecksumScope` bitset (plan §6.5).
    pub scope: u8,
    /// Server receive time.
    pub created_at: Timestamp,
}

/// Server-only checksum rate gate (plan §6.3: 1/2 s per identity).
#[table(accessor = checksum_rate)]
pub struct ChecksumRate {
    /// Caller.
    #[primary_key]
    pub identity: Identity,
    /// Current window start.
    pub window_start: Timestamp,
    /// Reports in the window.
    pub count: u32,
}

/// Records one scoped checksum for the caller (plan §3.7).
#[reducer]
pub fn publish_checksum(
    ctx: &ReducerContext,
    match_id: u64,
    command_id: u64,
    sim_tick: u64,
    checksum: u64,
    checksum_version: u32,
    scope: u8,
) -> Result<(), String> {
    let result = publish_checksum_impl(
        ctx,
        match_id,
        command_id,
        sim_tick,
        checksum,
        checksum_version,
        scope,
    );
    if let Err(error) = &result {
        log::warn!("publish_checksum rejected for match {match_id}: {error}");
    }
    result
}

fn publish_checksum_impl(
    ctx: &ReducerContext,
    match_id: u64,
    command_id: u64,
    sim_tick: u64,
    checksum: u64,
    checksum_version: u32,
    scope: u8,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    if row.status == MatchStatus::Ended {
        return Err(format!("match {match_id} has ended"));
    }
    require_member(ctx, match_id)?;
    if let Err(error) = validate_checksum(checksum_version, scope) {
        return Err(error);
    }
    rate_limit_checksum(ctx)?;
    ctx.db.match_checksum().insert(MatchChecksum {
        checksum_id: 0,
        match_id,
        sender: ctx.sender(),
        command_id,
        sim_tick,
        checksum,
        checksum_version,
        scope,
        created_at: ctx.timestamp,
    });
    Ok(())
}

/// Validation shared by the reducer and tests (plan §6.5).
pub fn validate_checksum(checksum_version: u32, scope: u8) -> Result<(), String> {
    if checksum_version != CHECKSUM_VERSION {
        return Err(format!(
            "checksum_version {checksum_version} != module {CHECKSUM_VERSION}"
        ));
    }
    if scope & !0b111 != 0 {
        return Err(format!("checksum scope {scope} has unknown bits"));
    }
    Ok(())
}

fn rate_limit_checksum(ctx: &ReducerContext) -> Result<(), String> {
    let identity = ctx.sender();
    let now = ctx.timestamp;
    let now_micros = now.to_micros_since_unix_epoch();
    let existing = ctx.db.checksum_rate().identity().find(identity);
    let mut rate = existing.unwrap_or(ChecksumRate {
        identity,
        window_start: now,
        count: 0,
    });
    if window_expired(
        now_micros,
        rate.window_start.to_micros_since_unix_epoch(),
        CHECKSUM_RATE_WINDOW_MS,
    ) {
        rate.window_start = now;
        rate.count = 0;
    }
    if rate.count >= CHECKSUM_RATE_MAX {
        return Err(format!(
            "checksum rate exceeded ({CHECKSUM_RATE_MAX}/{CHECKSUM_RATE_WINDOW_MS} ms)"
        ));
    }
    rate.count += 1;
    if ctx.db.checksum_rate().identity().find(identity).is_some() {
        ctx.db.checksum_rate().identity().update(rate);
    } else {
        ctx.db.checksum_rate().insert(rate);
    }
    Ok(())
}

/// A single peer's report, detached from the table for pure comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChecksumVote {
    /// Reporting peer.
    pub sender: Identity,
    /// Command watermark.
    pub command_id: u64,
    /// Reported value.
    pub checksum: u64,
    /// Reported version.
    pub checksum_version: u32,
}

/// A detected disagreement at one `command_id` (plan §3.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesyncDetected {
    /// Watermark the peers disagree on.
    pub command_id: u64,
    /// The host's value when the host voted at this watermark.
    pub host_checksum: Option<u64>,
    /// Value held by the most peers.
    pub majority: u64,
    /// `(sender, checksum)` pairs, sorted by sender.
    pub values: Vec<(Identity, u64)>,
}

impl DesyncDetected {
    /// The value correction should target: the host's, else the majority.
    pub fn canonical(&self) -> u64 {
        self.host_checksum.unwrap_or(self.majority)
    }
}

/// The correction a client must apply at `command_id` (plan §3.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Correction {
    /// Watermark to resync to.
    pub command_id: u64,
    /// Canonical value to adopt.
    pub canonical: u64,
    /// The outlier value the local peer had.
    pub ours: u64,
}

/// Groups votes by `command_id` and returns the first version-consistent
/// disagreement. Rows whose version differs from the reference version are
/// ignored (`version_mismatch_ignored`). `host` sharpens the canonical value.
pub fn compare_at_command_id(
    votes: &[ChecksumVote],
    host: Option<Identity>,
) -> Option<DesyncDetected> {
    let mut groups: BTreeMap<u64, Vec<&ChecksumVote>> = BTreeMap::new();
    for vote in votes {
        groups.entry(vote.command_id).or_default().push(vote);
    }
    for (command_id, group) in groups {
        if group.len() < 2 {
            continue;
        }
        // Reference version: the host's when present, else the first vote.
        let reference_version = host
            .and_then(|host| {
                group
                    .iter()
                    .find(|vote| vote.sender == host)
                    .map(|vote| vote.checksum_version)
            })
            .unwrap_or(group[0].checksum_version);
        let consistent: Vec<&&ChecksumVote> = group
            .iter()
            .filter(|vote| vote.checksum_version == reference_version)
            .collect();
        if consistent.len() < 2 {
            continue;
        }
        let distinct: std::collections::BTreeSet<u64> =
            consistent.iter().map(|vote| vote.checksum).collect();
        if distinct.len() < 2 {
            continue;
        }
        // Majority by count; ties resolved by the lowest value for determinism.
        let mut counts: BTreeMap<u64, usize> = BTreeMap::new();
        for vote in &consistent {
            *counts.entry(vote.checksum).or_default() += 1;
        }
        let majority = counts
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
            .map(|(value, _)| *value)
            .unwrap_or(0);
        let host_checksum = host.and_then(|host| {
            consistent
                .iter()
                .find(|vote| vote.sender == host)
                .map(|vote| vote.checksum)
        });
        let values: Vec<(Identity, u64)> = consistent
            .iter()
            .map(|vote| (vote.sender, vote.checksum))
            .collect();
        return Some(DesyncDetected {
            command_id,
            host_checksum,
            majority,
            values,
        });
    }
    None
}

/// Computes the correction for a local `ours` value, or `None` when already
/// canonical (plan §3.7).
pub fn host_canonical_correction(desync: &DesyncDetected, ours: u64) -> Option<Correction> {
    let canonical = desync.canonical();
    if ours == canonical {
        None
    } else {
        Some(Correction {
            command_id: desync.command_id,
            canonical,
            ours,
        })
    }
}

/// Whether a vote set is only version-divergent (no actionable desync).
pub fn version_mismatch_ignored(votes: &[ChecksumVote], host: Option<Identity>) -> bool {
    compare_at_command_id(votes, host).is_none()
}

/// Cadence in ticks advertised in `server_config` (kept referenced).
pub const _CHECKSUM_INTERVAL_TICKS: u64 = CHECKSUM_INTERVAL_TICKS;

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: u8) -> Identity {
        Identity::from_byte_array([byte; 32])
    }

    fn vote(sender: u8, command_id: u64, checksum: u64, version: u32) -> ChecksumVote {
        ChecksumVote {
            sender: id(sender),
            command_id,
            checksum,
            checksum_version: version,
        }
    }

    #[test]
    fn compare_at_command_id() {
        // Agreement: no desync.
        assert_eq!(
            super::compare_at_command_id(&[vote(1, 128, 7, 1), vote(2, 128, 7, 1)], Some(id(1))),
            None
        );
        // Disagreement at the same watermark.
        let votes = [vote(1, 128, 7, 1), vote(2, 128, 9, 1)];
        let desync = super::compare_at_command_id(&votes, Some(id(1))).expect("desync");
        assert_eq!(desync.command_id, 128);
        assert_eq!(desync.host_checksum, Some(7));
        assert_eq!(desync.canonical(), 7);
        // Different command ids do not compare.
        assert_eq!(
            super::compare_at_command_id(&[vote(1, 64, 7, 1), vote(2, 128, 9, 1)], Some(id(1))),
            None
        );
    }

    #[test]
    fn host_canonical_correction() {
        let votes = [
            vote(1, 256, 11, 1),
            vote(2, 256, 22, 1),
            vote(3, 256, 22, 1),
        ];
        let desync = super::compare_at_command_id(&votes, Some(id(1))).expect("desync");
        // Host is the outlier; majority is 22, canonical stays the host's 11.
        assert_eq!(desync.majority, 22);
        assert_eq!(desync.canonical(), 11);
        assert_eq!(
            super::host_canonical_correction(&desync, 22),
            Some(Correction {
                command_id: 256,
                canonical: 11,
                ours: 22
            })
        );
        // Already canonical: no correction.
        assert_eq!(super::host_canonical_correction(&desync, 11), None);
        // Without a host vote the majority becomes canonical.
        let no_host = super::compare_at_command_id(&votes, None).expect("desync");
        assert_eq!(no_host.canonical(), 22);
    }

    #[test]
    fn version_mismatch_ignored() {
        // Same value, different versions: no desync.
        let votes = [vote(1, 512, 5, 1), vote(2, 512, 5, 2)];
        assert_eq!(super::compare_at_command_id(&votes, None), None);
        // Different values but different versions: still ignored.
        let votes = [vote(1, 512, 5, 1), vote(2, 512, 6, 2)];
        assert!(super::version_mismatch_ignored(&votes, None));
        // A third peer on version 1 disagrees with peer 1, so it fires.
        let votes = [vote(1, 512, 5, 1), vote(3, 512, 6, 1), vote(2, 512, 9, 2)];
        assert!(!super::version_mismatch_ignored(&votes, None));
    }

    #[test]
    fn checksum_validation_caps_scope_and_version() {
        assert!(validate_checksum(CHECKSUM_VERSION, 0b001).is_ok());
        assert!(validate_checksum(CHECKSUM_VERSION + 1, 0b001).is_err());
        assert!(validate_checksum(CHECKSUM_VERSION, 0b1000).is_err());
    }
}
