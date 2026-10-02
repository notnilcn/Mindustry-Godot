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

#[test]
fn weapon_volley_matches_golden() {
    use mind_core::content::registries::units::ResolvedBullet;
    use mind_core::content::registries::units::weapon::{ShootPatternSpec, WeaponDef, WeaponSpec};

    let mut harness = CombatHarness::new(48, 16, 31);
    let wall = harness.content().block_id("copper-wall").expect("wall");
    for x in 12..=13 {
        assert!(harness.place(x, 8, wall, 0, true));
    }
    let bullet = harness.bullet_id("fuse").expect("fuse");
    let registry = harness.content();
    let weapon = WeaponDef::from_spec(
        WeaponSpec {
            name: "volley",
            reload: Some(10.0),
            x: Some(0.0),
            shoot_y: Some(0.0),
            recoil: Some(0.0),
            rotate: Some(false),
            mirror: Some(false),
            alternate: Some(false),
            shoot: Some(ShootPatternSpec::plain(1, 0.0, 0.0)),
            ..WeaponSpec::default()
        },
        ResolvedBullet {
            id: bullet,
            range: 200.0,
            heals: false,
            kill_shooter: false,
            dps: 0.0,
        },
        registry,
    )
    .expect("weapon");
    let (ux, uy) = CombatHarness::tile_center(4, 8);
    let unit = harness.spawn_test_unit(ux, uy, 1, vec![weapon]);
    harness.set_unit_aim(unit, 0, CombatHarness::tile_center(12, 8));
    harness.set_unit_shoot(unit, 0, true);
    for _ in 0..120 {
        harness.tick();
    }
    assert_eq!(
        harness.unit_weapons(unit).expect("unit weapons").mounts[0].total_shots,
        12
    );
    assert_eq!(harness.checksum_hex(), scenario("combat_weapon_volley"));
}

#[test]
fn turret_ammo_fire_matches_golden() {
    let mut harness = CombatHarness::new(48, 16, 37);
    let wall = harness.content().block_id("copper-wall").expect("wall");
    assert!(harness.place(12, 8, wall, 0, true));
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = harness
        .spawn_test_turret("duo", tx, ty, 1)
        .expect("duo turret");
    let copper = harness.content().item_id("copper").expect("copper");
    for _ in 0..10 {
        mind_core::world::blocks::defense::turrets::handle_item(
            &mut harness.build.world,
            turret,
            copper,
        );
    }
    for _ in 0..120 {
        harness.tick();
    }
    assert_eq!(harness.checksum_hex(), scenario("combat_turret_ammo"));
}
