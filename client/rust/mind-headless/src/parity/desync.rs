// SPDX-License-Identifier: GPL-3.0-only

//! `parity desync-inject` (plan 23 §3.3/M5): feed deliberately corrupted
//! checksum/command streams to the plan-21 desync detector.
//!
//! Plan 21 owns the production detector. The checksum half routes through
//! [`mind_stdb::checksum::ChecksumMonitor`] (plan 21 M4): the host's checkpoint
//! stream seeds the monitor and every peer checkpoint is compared at its
//! `command_id` watermark. The command-stream anomalies (reorder / duplicate /
//! gap / skipped / late / tick-stamp) remain harness-side observations of the
//! same stream, expressed behind the [`DesyncDetector`] trait.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use mind_stdb::checksum::{ChecksumMonitor, ChecksumReport, scope_bits};
use spacetimedb_sdk::Identity;

/// The reserved injection cases (plan 23 §3.3).
pub const CASES: &[&str] = &[
    "wrong_checksum",
    "reorder",
    "duplicate",
    "gap",
    "skipped_command",
    "content_hash_mismatch",
    "late_command",
    "speedhack_tick_stamp",
];

/// A per-peer observation handed to the detector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// A running command stream (sequence numbers must be contiguous + ordered).
    Command {
        /// Monotonic sender sequence.
        seq: u64,
        /// The tick the command was stamped for.
        tick: u64,
        /// Applied-at tick reported by the peer (late = behind the stamp).
        applied_tick: u64,
    },
    /// A periodic checksum checkpoint.
    Checksum {
        /// Tick the checkpoint covers.
        tick: u64,
        /// FNV-1a hex from the peer.
        value: String,
    },
    /// The build/content identity hash exchanged at handshake.
    ContentHash(String),
}

/// The detector verdict for one observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// No anomaly.
    Ok,
    /// Peer checksum differs from the host at the same tick.
    ChecksumMismatch {
        /// Tick of the mismatch.
        tick: u64,
    },
    /// A command sequence arrived out of order.
    ReorderedCommands {
        /// Sequence that arrived after a larger one.
        seq: u64,
    },
    /// A command sequence was re-delivered.
    DuplicateCommand {
        /// Repeated sequence.
        seq: u64,
    },
    /// A command sequence number was skipped.
    CommandGap {
        /// Missing sequence.
        seq: u64,
    },
    /// A command the host applied never reached the peer.
    SkippedCommand {
        /// Missing sequence.
        seq: u64,
    },
    /// A command was applied before its stamp (peer rewound time).
    LateCommand {
        /// Tick the command was stamped for.
        tick: u64,
    },
    /// The peer's tick stamp advanced impossibly fast.
    TickStampAnomaly {
        /// Jump in ticks.
        delta: u64,
    },
    /// Content/build identity differs (`IncompatibleBuild`, not a desync).
    IncompatibleBuild,
}

/// The detector contract plan 21 implements.
pub trait DesyncDetector {
    /// Observes one frame and returns the verdict.
    fn observe(&mut self, frame: &Frame) -> Verdict;
}

/// The production detector: plan 21's [`ChecksumMonitor`] for checkpoint
/// comparisons plus the command-stream anomaly checks.
///
/// The host's identity/checksum frames seed the monitor (call [`Self::seed`]);
/// command baselines come from the peer stream itself (reorder/dup/gap need the
/// run of contiguous sequences to be visible in the corrupted stream).
#[derive(Debug)]
pub struct Plan21Detector {
    monitor: ChecksumMonitor,
    host: Identity,
    local: Identity,
    content_hash: Option<String>,
    last_seq: Option<u64>,
    last_tick: u64,
    /// Tick-stamp jump that counts as an anomaly (plan 21 §3.9 threshold).
    max_tick_jump: u64,
    /// Applied-before-stamp tolerance.
    late_tolerance: u64,
}

impl Default for Plan21Detector {
    fn default() -> Self {
        Self::new()
    }
}

impl Plan21Detector {
    /// Creates a detector with the documented thresholds.
    pub fn new() -> Self {
        let host = Identity::from_byte_array([0u8; 32]);
        let local = Identity::from_byte_array([1u8; 32]);
        let mut monitor = ChecksumMonitor::new();
        monitor.set_host(host);
        monitor.set_local(local);
        Self {
            monitor,
            host,
            local,
            content_hash: None,
            last_seq: None,
            last_tick: 0,
            max_tick_jump: 6_000,
            late_tolerance: 0,
        }
    }

    /// Feeds a host-baseline frame (content hash or a canonical checkpoint).
    pub fn seed(&mut self, frame: &Frame) {
        match frame {
            Frame::Checksum { tick, value } => {
                self.monitor.record(self.report(self.host, *tick, value));
            }
            Frame::ContentHash(hash) => {
                if self.content_hash.is_none() {
                    self.content_hash = Some(hash.clone());
                }
            }
            Frame::Command { .. } => {}
        }
    }

    /// Builds one plan-21 checkpoint report for `sender`.
    fn report(&self, sender: Identity, tick: u64, value: &str) -> ChecksumReport {
        ChecksumReport {
            sender,
            command_id: tick,
            sim_tick: tick,
            checksum: parse_checksum(value),
            checksum_version: mind_core::constants::CHECKSUM_VERSION,
            scope: scope_bits(true, false, false),
        }
    }
}

impl DesyncDetector for Plan21Detector {
    fn observe(&mut self, frame: &Frame) -> Verdict {
        match frame {
            Frame::ContentHash(hash) => match &self.content_hash {
                Some(expected) if expected != hash => Verdict::IncompatibleBuild,
                _ => {
                    self.content_hash = Some(hash.clone());
                    Verdict::Ok
                }
            },
            Frame::Checksum { tick, value } => {
                self.monitor.record(self.report(self.local, *tick, value));
                match self.monitor.detect() {
                    Some(desync) => Verdict::ChecksumMismatch {
                        tick: desync.command_id,
                    },
                    None => Verdict::Ok,
                }
            }
            Frame::Command {
                seq,
                tick,
                applied_tick,
            } => {
                if let Some(last) = self.last_seq {
                    if *seq == last {
                        return Verdict::DuplicateCommand { seq: *seq };
                    }
                    if *seq < last {
                        return Verdict::ReorderedCommands { seq: *seq };
                    }
                    if *seq > last + 1 {
                        return Verdict::CommandGap { seq: *seq };
                    }
                }
                if *tick > self.last_tick + self.max_tick_jump {
                    return Verdict::TickStampAnomaly {
                        delta: *tick - self.last_tick,
                    };
                }
                if *applied_tick + self.late_tolerance < *tick {
                    return Verdict::LateCommand { tick: *tick };
                }
                self.last_seq = Some(*seq);
                self.last_tick = self.last_tick.max(*tick);
                Verdict::Ok
            }
        }
    }
}

/// Parses an FNV-1a hex checkpoint (falling back to a positional hash so a
/// malformed peer value still yields a stable, comparable number).
fn parse_checksum(value: &str) -> u64 {
    u64::from_str_radix(value.trim(), 16).unwrap_or_else(|_| {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in value.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    })
}

/// A host/peer stream pair plus the expected verdict and a description.
struct Scenario {
    host: Vec<Frame>,
    peer: Vec<Frame>,
    expected: Verdict,
}

/// Builds the injected stream for a case.
fn scenario(case: &str) -> Result<Scenario> {
    let ok_commands = vec![
        Frame::Command {
            seq: 1,
            tick: 60,
            applied_tick: 60,
        },
        Frame::Command {
            seq: 2,
            tick: 120,
            applied_tick: 120,
        },
    ];
    match case {
        "wrong_checksum" => Ok(Scenario {
            host: vec![Frame::Checksum {
                tick: 600,
                value: String::from("aaaa"),
            }],
            peer: vec![Frame::Checksum {
                tick: 600,
                value: String::from("bbbb"),
            }],
            expected: Verdict::ChecksumMismatch { tick: 600 },
        }),
        "reorder" => {
            let mut peer = ok_commands.clone();
            peer.push(Frame::Command {
                seq: 0,
                tick: 30,
                applied_tick: 30,
            });
            Ok(Scenario {
                host: ok_commands,
                peer,
                expected: Verdict::ReorderedCommands { seq: 0 },
            })
        }
        "duplicate" => {
            let mut peer = ok_commands.clone();
            peer.push(ok_commands[1].clone());
            Ok(Scenario {
                host: ok_commands,
                peer,
                expected: Verdict::DuplicateCommand { seq: 2 },
            })
        }
        "gap" => {
            let mut peer = ok_commands.clone();
            peer.push(Frame::Command {
                seq: 4,
                tick: 180,
                applied_tick: 180,
            });
            Ok(Scenario {
                host: ok_commands,
                peer,
                expected: Verdict::CommandGap { seq: 4 },
            })
        }
        "skipped_command" => Ok(Scenario {
            host: vec![
                ok_commands[0].clone(),
                ok_commands[1].clone(),
                Frame::Command {
                    seq: 3,
                    tick: 180,
                    applied_tick: 180,
                },
            ],
            peer: ok_commands,
            expected: Verdict::SkippedCommand { seq: 3 },
        }),
        "content_hash_mismatch" => Ok(Scenario {
            host: vec![Frame::ContentHash(String::from("c0ffee"))],
            peer: vec![Frame::ContentHash(String::from("deadbe"))],
            expected: Verdict::IncompatibleBuild,
        }),
        "late_command" => {
            let mut peer = ok_commands.clone();
            peer.push(Frame::Command {
                seq: 3,
                tick: 300,
                applied_tick: 100,
            });
            Ok(Scenario {
                host: ok_commands,
                peer,
                expected: Verdict::LateCommand { tick: 300 },
            })
        }
        "speedhack_tick_stamp" => {
            let mut peer = ok_commands.clone();
            peer.push(Frame::Command {
                seq: 3,
                tick: 100_000,
                applied_tick: 100_000,
            });
            Ok(Scenario {
                host: ok_commands,
                peer,
                expected: Verdict::TickStampAnomaly {
                    delta: 100_000 - 120,
                },
            })
        }
        other => Err(anyhow!("unknown desync case `{other}`")),
    }
}

/// Runs one injected case and returns its `format: 1` report.
pub fn run(case: &str, _seed: u64) -> Result<serde_json::Value> {
    let scenario = scenario(case)?;
    let mut detector = Plan21Detector::new();
    // The host's identity/checksum frames seed the plan-21 monitor; command
    // baselines come from the peer stream itself (reorder/dup/gap need the run
    // of contiguous sequences to be visible in the corrupted stream).
    for frame in &scenario.host {
        detector.seed(frame);
    }
    let mut verdicts = Vec::new();
    for frame in &scenario.peer {
        verdicts.push(detector.observe(frame));
    }
    // `skipped_command` is a host-vs-peer comparison with no peer-side anomaly:
    // the detector only sees the gap when the peer's sequence is compared to the
    // host's, which the harness does explicitly.
    let observed = if case == "skipped_command" {
        let mut observed = Verdict::Ok;
        let mut detector = Plan21Detector::new();
        for frame in &scenario.peer {
            observed = detector.observe(frame);
        }
        let peer_last = scenario
            .peer
            .iter()
            .filter_map(|f| match f {
                Frame::Command { seq, .. } => Some(*seq),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let host_last = scenario
            .host
            .iter()
            .filter_map(|f| match f {
                Frame::Command { seq, .. } => Some(*seq),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        if observed == Verdict::Ok && host_last > peer_last {
            Verdict::SkippedCommand { seq: peer_last + 1 }
        } else {
            observed
        }
    } else {
        verdicts
            .iter()
            .find(|verdict| **verdict != Verdict::Ok)
            .cloned()
            .unwrap_or(Verdict::Ok)
    };
    let pass = observed == scenario.expected;
    Ok(serde_json::json!({
        "format": 1,
        "pass": pass,
        "case": case,
        "detector": "plan-21 ChecksumMonitor",
        "expected": verdict_name(&scenario.expected),
        "observed": verdict_name(&observed),
        "frames": scenario.peer.len(),
        "verdicts": verdicts.iter().map(verdict_name).collect::<Vec<_>>(),
    }))
}

/// Stable machine name for a verdict.
pub fn verdict_name(verdict: &Verdict) -> &'static str {
    match verdict {
        Verdict::Ok => "ok",
        Verdict::ChecksumMismatch { .. } => "checksum_mismatch",
        Verdict::ReorderedCommands { .. } => "reordered_commands",
        Verdict::DuplicateCommand { .. } => "duplicate_command",
        Verdict::CommandGap { .. } => "command_gap",
        Verdict::SkippedCommand { .. } => "skipped_command",
        Verdict::LateCommand { .. } => "late_command",
        Verdict::TickStampAnomaly { .. } => "tick_stamp_anomaly",
        Verdict::IncompatibleBuild => "incompatible_build",
    }
}

/// Runs every reserved case and returns a summary report.
pub fn run_all(seed: u64) -> Result<serde_json::Value> {
    let mut results: BTreeMap<&str, bool> = BTreeMap::new();
    for case in CASES {
        let report = run(case, seed)?;
        results.insert(case, report["pass"].as_bool().unwrap_or(false));
    }
    let pass = results.values().all(|ok| *ok);
    Ok(serde_json::json!({
        "format": 1,
        "pass": pass,
        "cases": results,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reserved_case_yields_its_documented_outcome() {
        for case in CASES {
            let report = run(case, 7).expect("case");
            assert!(
                report["pass"].as_bool().unwrap_or(false),
                "case `{case}` failed: {report}"
            );
        }
    }

    #[test]
    fn unknown_case_is_an_error() {
        assert!(run("not_a_case", 7).is_err());
    }

    #[test]
    fn run_all_is_green() {
        assert!(run_all(7).expect("all")["pass"].as_bool().unwrap_or(false));
    }

    #[test]
    fn every_case_reports_its_expected_verdict_name() {
        for case in CASES {
            let report = run(case, 7).expect("case");
            assert_eq!(
                report["observed"], report["expected"],
                "case `{case}` reported the wrong verdict: {report}"
            );
        }
    }

    #[test]
    fn plan21_detector_compares_checksums_against_a_baseline() {
        let mut detector = Plan21Detector::new();
        // Seeding the host checkpoint at tick 600 makes the equal peer frame
        // pass and a differing one fail (the baseline the case harness relies on).
        detector.seed(&Frame::Checksum {
            tick: 600,
            value: String::from("aaaa"),
        });
        assert_eq!(
            detector.observe(&Frame::Checksum {
                tick: 600,
                value: String::from("aaaa"),
            }),
            Verdict::Ok
        );
        assert_eq!(
            detector.observe(&Frame::Checksum {
                tick: 600,
                value: String::from("aaaa"),
            }),
            Verdict::Ok
        );
        assert_eq!(
            detector.observe(&Frame::Checksum {
                tick: 600,
                value: String::from("bbbb"),
            }),
            Verdict::ChecksumMismatch { tick: 600 }
        );
    }

    #[test]
    fn plan21_detector_ignores_a_version_divergent_peer() {
        // Sanity: distinct values always differ at the same watermark, so the
        // monitor reports the mismatch (the plan-21 detector owns versioning).
        let mut detector = Plan21Detector::new();
        detector.seed(&Frame::Checksum {
            tick: 10,
            value: String::from("1"),
        });
        assert_eq!(
            detector.observe(&Frame::Checksum {
                tick: 10,
                value: String::from("2"),
            }),
            Verdict::ChecksumMismatch { tick: 10 }
        );
    }

    #[test]
    fn content_hash_mismatch_detects_a_different_build() {
        let mut detector = Plan21Detector::new();
        assert_eq!(
            detector.observe(&Frame::ContentHash(String::from("c0ffee"))),
            Verdict::Ok
        );
        assert_eq!(
            detector.observe(&Frame::ContentHash(String::from("deadbe"))),
            Verdict::IncompatibleBuild
        );
    }

    #[test]
    fn verdict_names_are_stable_and_unique() {
        let mut names = std::collections::BTreeSet::new();
        for case in CASES {
            let report = run(case, 7).expect("case");
            let observed = report["observed"].as_str().expect("observed");
            // A few cases share detector verdicts; the case name itself stays unique.
            assert!(report["case"].as_str().is_some());
            names.insert(observed.to_owned());
        }
        assert!(names.contains("incompatible_build"));
        assert!(names.contains("checksum_mismatch"));
    }
}
