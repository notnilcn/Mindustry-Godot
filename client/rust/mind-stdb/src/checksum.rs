// SPDX-License-Identifier: GPL-3.0-only

//! Client-side checksum publication bookkeeping and desync comparison
//! (plan 21 §3.7/§6.5).
//!
//! Pure helpers mirror the server's `relay::checksum` comparison so the client
//! can identify a disagreement, honor the host's canonical value and ignore
//! version-mismatched peers before triggering a resync. [`ChecksumMonitor`]
//! buffers recent `my_match_checksums` rows for one match.

use std::collections::BTreeMap;

use spacetimedb_sdk::Identity;

/// A published checksum report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChecksumReport {
    /// Reporting peer.
    pub sender: Identity,
    /// Command watermark.
    pub command_id: u64,
    /// Diagnostic sim tick.
    pub sim_tick: u64,
    /// Reported value.
    pub checksum: u64,
    /// Plan-05 `CHECKSUM_VERSION`.
    pub checksum_version: u32,
    /// `ChecksumScope` bitset.
    pub scope: u8,
}

/// A detected disagreement at one `command_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesyncDetected {
    /// Watermark peers disagree on.
    pub command_id: u64,
    /// Host value when the host voted.
    pub host_checksum: Option<u64>,
    /// Value held by most peers.
    pub majority: u64,
    /// `(sender, checksum)` pairs.
    pub values: Vec<(Identity, u64)>,
}

impl DesyncDetected {
    /// Value correction should adopt (host's, else majority).
    pub fn canonical(&self) -> u64 {
        self.host_checksum.unwrap_or(self.majority)
    }
}

/// The correction a client applies after a desync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Correction {
    /// Watermark to resync to.
    pub command_id: u64,
    /// Canonical value to adopt.
    pub canonical: u64,
    /// Local outlier value.
    pub ours: u64,
}

/// Compares rows grouped by `command_id`, ignoring differing versions.
pub fn compare_at_command_id(
    votes: &[ChecksumReport],
    host: Option<Identity>,
) -> Option<DesyncDetected> {
    let mut groups: BTreeMap<u64, Vec<&ChecksumReport>> = BTreeMap::new();
    for vote in votes {
        groups.entry(vote.command_id).or_default().push(vote);
    }
    for (command_id, group) in groups {
        if group.len() < 2 {
            continue;
        }
        let reference_version = host
            .and_then(|host| {
                group
                    .iter()
                    .find(|vote| vote.sender == host)
                    .map(|vote| vote.checksum_version)
            })
            .unwrap_or(group[0].checksum_version);
        let consistent: Vec<&&ChecksumReport> = group
            .iter()
            .filter(|vote| vote.checksum_version == reference_version)
            .collect();
        if consistent.len() < 2 {
            continue;
        }
        let mut counts: BTreeMap<u64, usize> = BTreeMap::new();
        for vote in &consistent {
            *counts.entry(vote.checksum).or_default() += 1;
        }
        if counts.len() < 2 {
            continue;
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

/// The correction for a local `ours`, or `None` when canonical.
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

/// Whether a set only differs by `checksum_version` (no actionable desync).
pub fn version_mismatch_ignored(votes: &[ChecksumReport], host: Option<Identity>) -> bool {
    compare_at_command_id(votes, host).is_none()
}

/// `ChecksumScope` bitset mirror (plan §6.5).
pub fn scope_bits(world_cohort: bool, include_possessed: bool, include_view: bool) -> u8 {
    (world_cohort as u8) | ((include_possessed as u8) << 1) | ((include_view as u8) << 2)
}

/// Rolling desync detector over `my_match_checksums` rows.
#[derive(Debug, Clone, Default)]
pub struct ChecksumMonitor {
    local: Option<Identity>,
    host: Option<Identity>,
    reports: Vec<ChecksumReport>,
}

impl ChecksumMonitor {
    /// Empty monitor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the local identity (the "ours" value).
    pub fn set_local(&mut self, identity: Identity) {
        self.local = Some(identity);
    }

    /// Records the canonical host identity.
    pub fn set_host(&mut self, identity: Identity) {
        self.host = Some(identity);
    }

    /// Buffers one report, dropping the oldest rows past `cap`.
    pub fn record(&mut self, report: ChecksumReport) {
        const CAP: usize = 2048;
        self.reports.push(report);
        if self.reports.len() > CAP {
            let overflow = self.reports.len() - CAP;
            self.reports.drain(0..overflow);
        }
    }

    /// Detects a disagreement at or before the newest watermark.
    pub fn detect(&self) -> Option<DesyncDetected> {
        compare_at_command_id(&self.reports, self.host)
    }

    /// The correction the local peer should apply, if any.
    pub fn correction(&self) -> Option<Correction> {
        let desync = self.detect()?;
        let ours = self
            .local
            .and_then(|local| {
                desync
                    .values
                    .iter()
                    .find(|(sender, _)| *sender == local)
                    .map(|(_, checksum)| *checksum)
            })
            .unwrap_or(desync.canonical());
        host_canonical_correction(&desync, ours)
    }

    /// Drops every buffered report (resync/reconnect).
    pub fn clear(&mut self) {
        self.reports.clear();
    }

    /// Buffered report count.
    pub fn len(&self) -> usize {
        self.reports.len()
    }

    /// Whether no reports are buffered.
    pub fn is_empty(&self) -> bool {
        self.reports.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn id(byte: u8) -> Identity {
        Identity::from_byte_array([byte; 32])
    }

    fn report(sender: u8, command_id: u64, checksum: u64, version: u32) -> ChecksumReport {
        ChecksumReport {
            sender: id(sender),
            command_id,
            sim_tick: command_id,
            checksum,
            checksum_version: version,
            scope: super::scope_bits(true, false, false),
        }
    }

    #[test]
    fn compare_at_command_id() {
        assert_eq!(
            super::compare_at_command_id(
                &[report(1, 128, 7, 1), report(2, 128, 7, 1)],
                Some(id(1))
            ),
            None
        );
        let votes = [report(1, 128, 7, 1), report(2, 128, 9, 1)];
        let desync = super::compare_at_command_id(&votes, Some(id(1))).unwrap();
        assert_eq!(desync.command_id, 128);
        assert_eq!(desync.host_checksum, Some(7));
        assert_eq!(desync.canonical(), 7);
        assert_eq!(
            super::compare_at_command_id(&[report(1, 64, 7, 1), report(2, 128, 9, 1)], Some(id(1))),
            None
        );
    }

    #[test]
    fn host_canonical_correction() {
        let votes = [
            report(1, 256, 11, 1),
            report(2, 256, 22, 1),
            report(3, 256, 22, 1),
        ];
        let desync = super::compare_at_command_id(&votes, Some(id(1))).unwrap();
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
        assert_eq!(super::host_canonical_correction(&desync, 11), None);
    }

    #[test]
    fn version_mismatch_ignored() {
        assert_eq!(
            super::compare_at_command_id(&[report(1, 512, 5, 1), report(2, 512, 5, 2)], None),
            None
        );
        assert!(super::version_mismatch_ignored(
            &[report(1, 512, 5, 1), report(2, 512, 6, 2)],
            None
        ));
        assert!(!super::version_mismatch_ignored(
            &[
                report(1, 512, 5, 1),
                report(3, 512, 6, 1),
                report(2, 512, 9, 2)
            ],
            None
        ));
    }

    #[test]
    fn scope_bits() {
        assert_eq!(super::scope_bits(true, false, false), 0b001);
        assert_eq!(super::scope_bits(true, true, false), 0b011);
        assert_eq!(super::scope_bits(false, false, false), 0b000);
        assert_eq!(super::scope_bits(true, true, true), 0b111);
    }

    #[test]
    fn monitor_detects_and_corrects() {
        let mut monitor = ChecksumMonitor::new();
        monitor.set_local(id(2));
        monitor.set_host(id(1));
        monitor.record(report(1, 128, 7, 1));
        monitor.record(report(2, 128, 9, 1));
        let correction = monitor.correction().unwrap();
        assert_eq!(correction.canonical, 7);
        assert_eq!(correction.ours, 9);
        monitor.clear();
        assert!(monitor.is_empty());
    }
}
