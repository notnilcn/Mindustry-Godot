// SPDX-License-Identifier: GPL-3.0-only

//! Plan 07 committed block goldens (`tests/golden/blocks_families.json`).

#![allow(clippy::expect_used)]

use mind_core::world::BuildHarness;
use mind_core::world::config::ConfigValue;
use mind_core::world::modules::{ItemModule, PowerModule};
use serde_json::Value;

fn golden() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/blocks_families.json"
    ))
    .expect("blocks_families.json");
    serde_json::from_str(&text).expect("parse golden")
}

#[test]
fn family_counts_match_golden() {
    let golden = golden();
    let harness = BuildHarness::new(4, 4, 0);
    let mut families: std::collections::BTreeMap<&'static str, u64> = Default::default();
    for inst in harness.table().iter() {
        *families.entry(inst.kind_data.family_name()).or_default() += 1;
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
    assert_eq!(
        harness.checksum_hex(),
        golden["scenarios"]["consumer_efficiency"]
            .as_str()
            .expect("golden")
    );
}
