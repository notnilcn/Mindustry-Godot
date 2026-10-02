// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan 18 §7b committed audio-scenario goldens.
//!
//! Runs every `audio *` oracle through the compiled `mind-headless` binary and
//! asserts it compares clean against `tests/golden/audio/*`.

#![allow(clippy::expect_used)]

use std::process::Command;

const ARGS: [&[&str]; 5] = [
    &[
        "audio",
        "events",
        "--scenario",
        "audio_events_blocks",
        "--json",
    ],
    &[
        "audio",
        "events",
        "--scenario",
        "audio_events_sim",
        "--json",
    ],
    &["audio", "music", "--json"],
    &["audio", "loops", "--json"],
    &["audio", "policy", "--json"],
];

#[test]
fn audio_scenarios_match_committed_goldens() {
    let exe = env!("CARGO_BIN_EXE_mind-headless");
    for args in ARGS {
        let output = Command::new(exe)
            .args(args)
            .output()
            .unwrap_or_else(|error| panic!("{args:?}: spawn {error}"));
        assert!(
            output.status.success(),
            "{args:?} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
