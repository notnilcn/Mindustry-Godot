// SPDX-License-Identifier: GPL-3.0-only

//! Plan 15 committed input-scenario goldens.
//!
//! Each scenario in `mind_headless::input_scenarios` is re-run and compared
//! against its committed JSON under `tests/golden/input/`. The goldens are the
//! source of truth (plan 15 §7b); regenerate with
//! `mind-headless input scenario <name> --dump tests/golden/input/<file>`.

use std::path::{Path, PathBuf};

use mind_headless::cli::InputCommand;
use mind_headless::input_scenarios;

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/input")
        .join(name)
}

fn check(scenario: &str, golden_file: &str) {
    let command = InputCommand::Scenario {
        name: scenario.to_owned(),
        json: false,
        dump: None,
        golden: Some(golden(golden_file)),
    };
    let result = input_scenarios::run(&command);
    assert!(result.is_ok(), "scenario {scenario}: {result:?}");
}

#[test]
fn input_scenarios_match_committed_goldens() {
    for (scenario, file) in [
        ("input_focus_guards", "focus_guards.json"),
        (
            "placement_validation_table",
            "placement_validation_table.json",
        ),
        (
            "input_place_line_headless",
            "input_place_line_headless.json",
        ),
        ("input_replay_mobile", "input_replay_mobile.json"),
        ("input_mobile_parity", "input_mobile_parity.json"),
        ("input_rts_move", "input_rts_move.json"),
    ] {
        check(scenario, file);
    }
}
