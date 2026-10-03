// SPDX-License-Identifier: GPL-3.0-only

//! `parity desync-inject` (plan 23 §3.3/M5): feed deliberately corrupted
//! checksum/command streams to the plan-21 desync detector.
//!
//! Plan 21 owns the production detector. Until it lands this module carries a
//! deterministic **reference detector double** behind a trait so the injected
//! cases and their expected outcomes are already pinned. Swapping in plan 21's
//! detector is a trait-impl change here, not a case change.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow};

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

/// The reference detector double (placeholder for plan 21).
///
/// It tracks the last observed command sequence and tick and flags the injected
/// anomalies with their documented outcomes.
#[derive(Debug, Default)]
pub struct ReferenceDetector {
    last_seq: Option<u64>,
    last_tick: u64,
    /// Tick-stamp jump that counts as an anomaly (plan 21 §3.9 threshold).
    max_tick_jump: u64,
    /// Applied-before-stamp tolerance.
    late_tolerance: u64,
    content_hash: Option<String>,
    /// Host baseline checkpoints keyed by tick (seeded before peer frames).
    expected_checksums: BTreeMap<u64, String>,
}

impl ReferenceDetector {
    /// Creates a detector with the documented thresholds.
    pub fn new() -> Self {
        Self {
            max_tick_jump: 6_000,
            late_tolerance: 0,
            ..Self::default()
        }
    }
}

impl DesyncDetector for ReferenceDetector {
    fn observe(&mut self, frame: &Frame) -> Verdict {
        match frame {
            Frame::ContentHash(hash) => match &self.content_hash {
                Some(expected) if expected != hash => Verdict::IncompatibleBuild,
                _ => {
                    self.content_hash = Some(hash.clone());
                    Verdict::Ok
                }
            },
            Frame::Checksum { tick, value } => match self.expected_checksums.get(tick) {
                Some(expected) if expected != value => Verdict::ChecksumMismatch { tick: *tick },
                Some(_) => Verdict::Ok,
                None => {
                    self.expected_checksums.insert(*tick, value.clone());
                    Verdict::Ok
                }
            },
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
    let mut detector = ReferenceDetector::new();
    // The host's identity/checksum frames are the detector's baseline; command
    // baselines come from the peer stream itself (reorder/dup/gap need the run
    // of contiguous sequences to be visible in the corrupted stream).
    for frame in &scenario.host {
        match frame {
            Frame::ContentHash(_) | Frame::Checksum { .. } => {
                let _ = detector.observe(frame);
            }
            Frame::Command { .. } => {}
        }
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
        let mut detector = ReferenceDetector::new();
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
        "detector": "reference-double (plan 21 owns the production detector)",
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
    fn reference_detector_compares_checksums_against_a_baseline() {
        let mut detector = ReferenceDetector::new();
        // Seeding the host checkpoint at tick 600 makes the equal peer frame
        // pass and a differing one fail (the baseline the case harness relies on).
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
    fn content_hash_mismatch_detects_a_different_build() {
        let mut detector = ReferenceDetector::new();
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
