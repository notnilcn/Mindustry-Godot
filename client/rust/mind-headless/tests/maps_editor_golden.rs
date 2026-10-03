// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan 06/19 map-round-trip + editor-playtest committed goldens.
//!
//! Runs the `maps roundtrip` and `editor playtest` scenarios through the
//! compiled `mind-headless` binary and asserts the deterministic checksums
//! against `tests/golden/maps_editor.json` (plan 06 M4 / plan 19 M7).

#![allow(clippy::expect_used)]

use std::path::Path;
use std::process::Command;

fn golden() -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/maps_editor.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).expect("parse maps_editor golden")
}

fn run(args: &[&str]) -> serde_json::Value {
    let exe = env!("CARGO_BIN_EXE_mind-headless");
    let mut full = args.to_vec();
    full.extend_from_slice(&["--json", "--log-file", "-"]);
    let output = Command::new(exe)
        .args(&full)
        .output()
        .unwrap_or_else(|error| panic!("spawn {args:?}: {error}"));
    assert!(
        output.status.success(),
        "{args:?} failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{args:?}: bad json {error}"))
}

#[test]
fn maps_roundtrip_matches_golden() {
    let golden = golden();
    let report = run(&["maps", "roundtrip"]);
    assert_eq!(report["ok"].as_bool(), Some(true), "scenario failed");
    assert_eq!(
        report["preview_checksum_save"].as_str(),
        golden["maps_roundtrip"].as_str(),
        "map round-trip preview checksum mismatch"
    );
    assert_eq!(
        report["preview_checksum_import"].as_str(),
        golden["maps_roundtrip"].as_str(),
        "import preview checksum must equal save"
    );
}

#[test]
fn editor_playtest_matches_golden() {
    let golden = golden();
    let report = run(&["editor", "playtest"]);
    assert_eq!(report["ok"].as_bool(), Some(true), "scenario failed");
    assert_eq!(
        report["checksum"].as_str(),
        golden["editor_playtest"].as_str(),
        "editor playtest transition checksum mismatch"
    );
}
