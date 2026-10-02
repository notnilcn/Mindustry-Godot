// SPDX-License-Identifier: GPL-3.0-only

//! Plan 10 committed combat goldens (`tests/golden/combat.json`).
//!
//! The golden values are the deterministic `CombatHarness` checksums produced by
//! the `mind-headless combat scenario` fixtures; the CLI prints the same values.

#![allow(clippy::expect_used)]

use mind_core::combat::CombatHarness;
use serde_json::Value;

fn golden() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/combat.json"
    ))
    .expect("combat.json");
    serde_json::from_str(&text).expect("parse golden")
}

fn scenario(name: &str) -> String {
    golden()["scenarios"][name]
        .as_str()
        .expect("scenario golden")
        .to_owned()
}

#[test]
fn basic_bullet_hits_building_matches_golden() {
    let mut harness = CombatHarness::new(32, 32, 7);
    let wall = harness.content().block_id("copper-wall").expect("wall");
    assert!(harness.place(10, 10, wall, 0, true));
    let (x, y) = CombatHarness::tile_center(8, 10);
    assert!(harness.spawn_bullet("fuse", x, y, 0.0, 1).is_some());
    for _ in 0..120 {
        harness.tick();
    }
    assert_eq!(harness.building_health_at(10, 10), 280.0);
    assert_eq!(harness.checksum_hex(), scenario("combat_basic"));
}

#[test]
fn rail_pierces_three_walls_matches_golden() {
    let mut harness = CombatHarness::new(48, 16, 11);
    let wall = harness.content().block_id("copper-wall").expect("wall");
    for x in 12..=14 {
        assert!(harness.place(x, 8, wall, 0, true));
    }
    let (x, y) = CombatHarness::tile_center(4, 8);
    assert!(harness.spawn_bullet("rail", x, y, 0.0, 1).is_some());
    for _ in 0..80 {
        harness.tick();
    }
    assert_eq!(harness.checksum_hex(), scenario("combat_bullet_pierce"));
}

#[test]
fn determinism_samples_are_identical_and_match_golden() {
    fn sample(seed: u64) -> Vec<String> {
        let mut harness = CombatHarness::new(64, 64, seed);
        for i in 0..32u32 {
            let (x, y) = CombatHarness::tile_center(2 + (i % 8) as i32 * 6, 2 + (i / 8) as i32 * 6);
            let angle = (i as f32 * 37.0) % 360.0;
            let _ = harness.spawn_bullet("fuse_slow", x, y, angle, 1);
        }
        let mut samples = Vec::new();
        for tick in 1..=360u64 {
            harness.step_bullets_only();
            if tick % 60 == 0 {
                samples.push(harness.checksum_hex());
            }
        }
        samples
    }
    let first = sample(29);
    assert_eq!(first, sample(29));
    assert_eq!(first[0], scenario("combat_determinism_first"));
}
