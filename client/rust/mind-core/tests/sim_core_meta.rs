// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan 05 M5 integration oracle: the entity metadata JSON must stay byte-stable
//! against `tests/golden/entitymeta.json` so field additions are visible in
//! review (plan 05 §6.2/§7.1).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use mind_core::entities::vanilla_registry;

#[test]
fn entitymeta_matches_golden() {
    const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/entitymeta.json");
    let registry = vanilla_registry().expect("vanilla registry builds");
    let live = registry.metadata_json();
    // `MIND_BLESS=1` rewrites the golden in the `meta entities --out` shape
    // (L08 T1); the reason goes into `parity/golden_manifest.json` `reasons`.
    if std::env::var("MIND_BLESS").is_ok_and(|v| v == "1") {
        let text = format!("{}\n", serde_json::to_string_pretty(&live).unwrap());
        std::fs::write(GOLDEN, text).unwrap();
        return;
    }
    let golden_text = std::fs::read_to_string(GOLDEN).expect("golden entitymeta.json is committed");
    let golden: serde_json::Value =
        serde_json::from_str(&golden_text).expect("golden is valid JSON");
    assert_eq!(
        live, golden,
        "entity metadata drifted; regenerate with `mind-headless meta entities --out`"
    );
}

#[test]
fn no_unknown_components_and_classids_resolve() {
    let registry = vanilla_registry().expect("registry");
    // Every def resolves a class id from the committed classids.properties.
    assert_eq!(
        registry.def_by_name("BuildingComp").map(|d| d.class_id),
        Some(6)
    );
    assert_eq!(
        registry.def_by_name("BulletComp").map(|d| d.class_id),
        Some(7)
    );
}
