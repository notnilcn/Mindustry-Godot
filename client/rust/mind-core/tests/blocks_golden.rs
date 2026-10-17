// SPDX-License-Identifier: GPL-3.0-only

//! Plan 07 committed block goldens (`tests/golden/blocks_families.json`).
//!
//! `MIND_BLESS=1 cargo test -p mind-core --test blocks_golden` rewrites the
//! golden from the current build instead of asserting (L08 T1). A bless must
//! carry its reason (commit + Java citation) in `parity/golden_manifest.json`
//! `reasons` and the commit body; then re-record the manifest sha256 with
//! `tools/regen_goldens.sh --record harness_blocks_families`.

#![allow(clippy::expect_used)]

use std::sync::Mutex;

use mind_core::world::BuildHarness;
use mind_core::world::config::ConfigValue;
use mind_core::world::modules::{ItemModule, PowerModule};
use serde_json::Value;

const GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/blocks_families.json"
);

/// Serialises read-modify-write blesses across the parallel tests.
static BLESS_LOCK: Mutex<()> = Mutex::new(());

fn golden() -> Value {
    let text = std::fs::read_to_string(GOLDEN_PATH).expect("blocks_families.json");
    serde_json::from_str(&text).expect("parse golden")
}

fn blessing() -> bool {
    std::env::var("MIND_BLESS").is_ok_and(|v| v == "1")
}

/// Rewrites one top-level-or-nested golden field (`path` of object keys).
fn bless(path: &[&str], value: Value) {
    let _guard = BLESS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut root = golden();
    let mut node = &mut root;
    for key in &path[..path.len() - 1] {
        node = node.get_mut(*key).expect("golden path");
    }
    node[path[path.len() - 1]] = value;
    let mut text = serde_json::to_string_pretty(&root).expect("serialize golden");
    text.push('\n');
    std::fs::write(GOLDEN_PATH, text).expect("write golden");
}

#[test]
fn family_counts_match_golden() {
    let golden = golden();
    let harness = BuildHarness::new(4, 4, 0);
    let mut families: std::collections::BTreeMap<&'static str, u64> = Default::default();
    for inst in harness.table().iter() {
        *families.entry(inst.kind_data.family_name()).or_default() += 1;
    }
    if blessing() {
        bless(&["blocks"], Value::from(harness.table().len() as u64));
        bless(
            &["families"],
            serde_json::to_value(&families).expect("families"),
        );
        return;
    }
    assert_eq!(
        Some(harness.table().len() as u64),
        golden["blocks"].as_u64()
    );
    for (family, expected) in golden["families"].as_object().expect("families") {
        assert_eq!(
            families.get(family.as_str()).copied().unwrap_or(0),
            expected.as_u64().unwrap_or(0),
            "family `{family}`"
        );
    }
}

#[test]
fn wall_door_checksum_matches_golden() {
    let golden = golden();
    let mut harness = BuildHarness::new(32, 32, 7);
    let wall = harness.content().block_id("copper-wall").expect("wall");
    let door = harness.content().block_id("door").expect("door");
    harness.place(4, 4, wall, 0, true);
    harness.place(5, 4, door, 0, false);
    harness.construct_tick(10_000.0);
    harness.configure(5, 4, ConfigValue::Bool(true));
    harness.configure(5, 4, ConfigValue::Bool(false));
    if blessing() {
        bless(
            &["scenarios", "wall_door"],
            Value::from(harness.checksum_hex()),
        );
        return;
    }
    assert_eq!(
        harness.checksum_hex(),
        golden["scenarios"]["wall_door"].as_str().expect("golden")
    );
}

#[test]
fn consumer_efficiency_checksum_matches_golden() {
    let golden = golden();
    let mut harness = BuildHarness::new(32, 32, 7);
    let smelter = harness
        .content()
        .block_id("silicon-smelter")
        .expect("silicon-smelter");
    harness.place(4, 4, smelter, 0, true);
    let entity = harness.build_at(4, 4).expect("building");
    let coal = harness.content().item_id("coal").expect("coal");
    let sand = harness.content().item_id("sand").expect("sand");
    {
        let mut items = harness.world.get_mut::<ItemModule>(entity).expect("items");
        items.add(coal, 100, 100);
        items.add(sand, 100, 100);
    }
    if let Some(mut power) = harness.world.get_mut::<PowerModule>(entity) {
        power.status = 1.0;
    }
    for _ in 0..90 {
        harness.tick();
    }
    if blessing() {
        bless(
            &["scenarios", "consumer_efficiency"],
            Value::from(harness.checksum_hex()),
        );
        return;
    }
    assert_eq!(
        harness.checksum_hex(),
        golden["scenarios"]["consumer_efficiency"]
            .as_str()
            .expect("golden")
    );
}
