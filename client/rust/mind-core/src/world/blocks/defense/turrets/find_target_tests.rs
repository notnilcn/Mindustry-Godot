// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Turret.findTarget` unit-sort and tracking-range tests (J04 T1).
//! Upstream: none (port-only; turret target tests)

use super::tests::set_unit_elevation;
use super::types::{turret as state_of, turret_mut as state_of_mut};
use super::*;
use crate::combat::harness::CombatHarness;
use crate::entities::comp::Health;
use crate::entities::comp::Pos;
use crate::world::construct::ConstructState;
use crate::world::modules::LiquidModule;
use bevy_ecs::entity::Entity;

/// Spawns a turret with one ammo entry (`amount` units) and a zeroed
/// retarget timer so the next pass acquires a target.
fn armed_turret(
    harness: &mut CombatHarness,
    name: &str,
    x: f32,
    y: f32,
    item_name: &str,
    bullet_name: &str,
) -> Entity {
    let turret = harness
        .spawn_test_turret(name, x, y, 1)
        .unwrap_or_else(|| panic!("turret `{name}`"));
    let item = harness.content().item_id(item_name).expect(item_name);
    let bullet = harness.content().bullet_names()[bullet_name];
    let state = state_of_mut(&mut harness.build.world, turret).expect("turret state");
    state.ammo.push(AmmoEntry {
        item,
        bullet,
        amount: 40,
    });
    state.target_timer = 0.0;
    turret
}

/// Two enemies at different health and distance: foreshadow (`strongest`,
/// `Blocks.java:4261`) picks the far high-health unit, duo (`closest`,
/// `Turret.java:113` default) the near one.
#[test]
fn find_target_strongest_and_closest_sorts() {
    for (turret_name, item_name, bullet_name, expect_far) in [
        ("foreshadow", "surge-alloy", "foreshadow_rail", true),
        ("duo", "copper", "duo_copper", false),
    ] {
        let mut harness = CombatHarness::new(64, 64, 7);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = armed_turret(&mut harness, turret_name, tx, ty, item_name, bullet_name);
        // Near unit (40 px, 100 health) vs far unit (80 px, 1000 health);
        // `strongest = -maxHealth + dst2/6400` (`UnitSorts.java:13`).
        let near = harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
        harness
            .build
            .world
            .get_mut::<Health>(near)
            .unwrap()
            .max_health = 100.0;
        let far = harness.spawn_test_unit(tx + 80.0, ty, 2, Vec::new());
        harness
            .build
            .world
            .get_mut::<Health>(far)
            .unwrap()
            .max_health = 1000.0;
        harness.update_turrets_only();
        let state = harness.turret_state(turret).expect("state");
        let expected = if expect_far { far } else { near };
        assert_eq!(state.target, Some(expected), "`{turret_name}` target sort");
    }
}

/// Ripple (`unitSort = grouped`, `Blocks.java:4107`) picks a member of the
/// clustered trio over a lone nearer unit (`-getClustered + dst * 0.003`,
/// `UnitSorts.java:24-27`).
#[test]
fn find_target_grouped_prefers_cluster() {
    let mut harness = CombatHarness::new(64, 64, 7);
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = armed_turret(
        &mut harness,
        "ripple",
        tx,
        ty,
        "graphite",
        "ripple_graphite",
    );
    // Lone unit 40 px away: cost -1 + 40 * 0.003 = -0.88.
    harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
    // Three units in one 70 px cluster cell 200 px away: cost -3 + 0.6.
    let first_cluster = harness.spawn_test_unit(tx + 200.0, ty, 2, Vec::new());
    harness.spawn_test_unit(tx + 201.0, ty, 2, Vec::new());
    harness.spawn_test_unit(tx + 202.0, ty, 2, Vec::new());
    harness.update_turrets_only();
    let state = harness.turret_state(turret).expect("state");
    assert_eq!(
        state.target,
        Some(first_cluster),
        "grouped picks the cluster (first in stable unit order on a tie)"
    );
}

/// Ripple's `minRange = 50` (`Blocks.java:4100`) clamps the `scaleLife`
/// lifetime to `minRange()/range` (`Turret.java:797`): a shell aimed 40 px
/// away lives for the minimum fraction, not the shorter raw distance ratio.
#[test]
fn ripple_min_range_clamps_bullet_lifetime() {
    let mut harness = CombatHarness::new(64, 64, 7);
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = armed_turret(
        &mut harness,
        "ripple",
        tx,
        ty,
        "graphite",
        "ripple_graphite",
    );
    // Target inside the 50-unit minimum range.
    let target = harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
    {
        let state = state_of_mut(&mut harness.build.world, turret).unwrap();
        state.target = Some(target);
        state.target_pos = (tx + 40.0, ty);
    }
    let def_lifetime = {
        let bullet = harness.content().bullet_names()["ripple_graphite"];
        harness
            .content()
            .bullet(bullet)
            .map(|def| def.lifetime)
            .unwrap_or(0.0)
    };
    let mut fired: Option<f32> = None;
    for _ in 0..200 {
        harness.tick();
        if
        // A newer bullet than the pre-set state: the first spawn.
        let Some(entity) = harness.bullets.last().copied()
            && let Some(bullet) = harness
                .build
                .world
                .get::<crate::combat::bullet::Bullet>(entity)
        {
            fired = Some(bullet.lifetime);
            break;
        }
    }
    let lifetime = fired.expect("a shell spawned");
    // (baseLife(1) + scaleLifetimeOffset(0.1)) * dst(40) / type.range(240)
    // = 0.1833, clamped up to minRange(50)/range(240) = 0.2083.
    let expected = def_lifetime * (50.0 / 240.0);
    assert!(
        (lifetime - expected).abs() < 0.5,
        "clamped lifetime {lifetime} ~= {expected}"
    );
    let unclamped = def_lifetime * 1.1 * 40.0 / 240.0;
    assert!(
        lifetime > unclamped,
        "the min-range clamp raised the lifetime"
    );
}

/// Smite's `trackingRange = range * 1.4` (`Blocks.java:5947`): a target
/// beyond the firing range but inside the tracking range is acquired; one
/// beyond it is not.
#[test]
fn find_target_uses_tracking_range() {
    let mut harness = CombatHarness::new(128, 64, 7);
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = armed_turret(&mut harness, "smite", tx, ty, "surge-alloy", "smite_orb");
    let config = harness.turret_state(turret).expect("state").config.clone();
    assert!((config.tracking_range - config.range * 1.4).abs() < 0.01);
    // 350 px: outside `range` 300, inside `trackingRange` 420.
    let tracked = harness.spawn_test_unit(tx + 350.0, ty, 2, Vec::new());
    harness.update_turrets_only();
    assert_eq!(
        harness.turret_state(turret).unwrap().target,
        Some(tracked),
        "second pass at trackingRange()"
    );
    // 460 px: outside the tracking range too.
    harness.build.world.get_mut::<Pos>(tracked).unwrap().x = tx + 460.0;
    state_of_mut(&mut harness.build.world, turret)
        .unwrap()
        .target_timer = 0.0;
    harness.update_turrets_only();
    assert_eq!(
        harness.turret_state(turret).unwrap().target,
        None,
        "out of tracking range"
    );
}

/// `Turret.newTargetInterval` (`Turret.java:571`): scathe retargets on 40
/// ticks while holding a target (`Blocks.java:5666`), the plain interval
/// otherwise.
#[test]
fn new_target_interval_applies_with_a_target() {
    let mut harness = CombatHarness::new(64, 64, 7);
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = armed_turret(&mut harness, "scathe", tx, ty, "carbide", "scathe_carbide");
    assert_eq!(
        harness
            .turret_state(turret)
            .unwrap()
            .config
            .new_target_interval,
        40.0
    );
    // First pass: no target held -> the plain interval.
    harness.update_turrets_only();
    assert_eq!(harness.turret_state(turret).unwrap().target, None);
    assert_eq!(
        harness.turret_state(turret).unwrap().target_timer,
        harness.turret_state(turret).unwrap().config.target_interval
    );
    // With a target in range the retarget interval becomes 40.
    harness.spawn_test_unit(tx + 100.0, ty, 2, Vec::new());
    state_of_mut(&mut harness.build.world, turret)
        .unwrap()
        .target_timer = 0.0;
    harness.update_turrets_only();
    assert!(harness.turret_state(turret).unwrap().target.is_some());
    assert_eq!(harness.turret_state(turret).unwrap().target_timer, 40.0);
}

/// `Turret.findEnemy` air-only branch (`Turret.java:644-646`): scatter
/// (`targetGround = false`) rejects grounded units
/// (`isGrounded = elevation < 0.001`, `UnitComp.java:80-82`) and targets
/// aircraft. The accept predicate is `!e.isGrounded()` — not the
/// `isFlying()` (`elevation >= 0.09`) band.
#[test]
fn air_only_turret_targets_aircraft_over_ground() {
    let mut harness = CombatHarness::new(64, 64, 7);
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = armed_turret(&mut harness, "scatter", tx, ty, "scrap", "scatter_scrap");
    let config = harness.turret_state(turret).expect("state").config.clone();
    assert!(
        config.target_air && !config.target_ground,
        "scatter is air-only"
    );
    // Grounded enemy 40 px away: nearest, but not a legal target.
    harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
    // Flying enemy 100 px away: the only legal target.
    let flying = harness.spawn_test_unit(tx + 100.0, ty, 2, Vec::new());
    set_unit_elevation(&mut harness, flying, 1.0, true);
    harness.update_turrets_only();
    assert_eq!(
        harness.turret_state(turret).expect("state").target,
        Some(flying),
        "`!isGrounded()` keeps the aircraft and drops the grounded unit"
    );
}

/// Air-only turret with only grounded enemies in range: no target
/// (`Turret.java:645` rejects every `isGrounded()` unit).
#[test]
fn air_only_turret_rejects_grounded_only() {
    let mut harness = CombatHarness::new(64, 64, 7);
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = armed_turret(&mut harness, "scatter", tx, ty, "scrap", "scatter_scrap");
    harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
    harness.spawn_test_unit(tx + 120.0, ty, 2, Vec::new());
    harness.update_turrets_only();
    assert_eq!(harness.turret_state(turret).expect("state").target, None);
}

/// Java's air-only gate is `!isGrounded()` (`elevation < 0.001`), so a unit
/// in the 0.001..0.09 band is a legal target even though `isFlying()`
/// (`elevation >= 0.09`, `UnitComp.java:84-86`) is false — the air-only
/// branch must not be written as `isFlying()`.
#[test]
fn air_only_turret_accepts_mid_band_elevation() {
    let mut harness = CombatHarness::new(64, 64, 7);
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = armed_turret(&mut harness, "scatter", tx, ty, "scrap", "scatter_scrap");
    let band = harness.spawn_test_unit(tx + 60.0, ty, 2, Vec::new());
    set_unit_elevation(&mut harness, band, 0.05, false);
    harness.update_turrets_only();
    assert_eq!(
        harness.turret_state(turret).expect("state").target,
        Some(band),
        "0.05 elevation is not grounded, so the air-only branch keeps it"
    );
}

/// General branch (`Turret.java:647-651`): a `targetGround && targetAir`
/// turret keeps both bands — `(isGrounded() || targetAir) &&
/// (!isGrounded() || targetGround)`.
#[test]
fn general_turret_targets_ground_and_air() {
    for (elevation, physics_flying) in [(0.0, false), (1.0, true)] {
        let mut harness = CombatHarness::new(64, 64, 7);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = armed_turret(&mut harness, "duo", tx, ty, "copper", "duo_copper");
        let config = harness.turret_state(turret).expect("state").config.clone();
        assert!(
            config.target_air && config.target_ground,
            "duo targets both"
        );
        let unit = harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
        set_unit_elevation(&mut harness, unit, elevation, physics_flying);
        harness.update_turrets_only();
        assert_eq!(
            harness.turret_state(turret).expect("state").target,
            Some(unit),
            "targetAir && targetGround keeps elevation {elevation}"
        );
    }
}

/// General branch with `targetAir = false` (hail, `Blocks.java`): a unit in
/// the 0.001..0.09 band is not grounded, so Java rejects it in both terms
/// (`(isGrounded() || targetAir)` is false and `!isGrounded()` alone is not
/// enough without the first term).
#[test]
fn ground_only_turret_rejects_mid_band_elevation() {
    let mut harness = CombatHarness::new(64, 64, 7);
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = armed_turret(&mut harness, "hail", tx, ty, "graphite", "hail_graphite");
    let config = harness.turret_state(turret).expect("state").config.clone();
    assert!(
        config.target_ground && !config.target_air,
        "hail is ground-only"
    );
    let band = harness.spawn_test_unit(tx + 60.0, ty, 2, Vec::new());
    set_unit_elevation(&mut harness, band, 0.05, false);
    harness.update_turrets_only();
    assert_eq!(
        harness.turret_state(turret).expect("state").target,
        None,
        "0.05 elevation is not grounded and hail has targetAir = false"
    );
    // A truly grounded unit at the same distance is still targeted.
    let grounded = harness.spawn_test_unit(tx + 60.0, ty, 2, Vec::new());
    state_of_mut(&mut harness.build.world, turret)
        .expect("state")
        .target_timer = 0.0;
    harness.update_turrets_only();
    let _ = band;
    assert_eq!(
        harness.turret_state(turret).expect("state").target,
        Some(grounded)
    );
}

// ---- GAPS #2: Turret.limitRange -------------------------------------

/// `Turret.limitRange` (`Turret.java:246-253`): every ammo bullet's
/// lifetime is recomputed from the turret range, its `rangeChange`, the
/// per-turret margin and `extraRangeMargin`.
#[test]
fn limit_range_recomputes_ammo_lifetime() {
    let harness = CombatHarness::new(16, 16, 7);
    let lifetime = |name: &str| {
        let id = harness.content().bullet_by_name(name).expect(name);
        harness.content().bullet(id).expect(name).lifetime
    };
    // `Blocks.java:3348` duo `limitRange(5f)`: (160 + 5 + 10) / 2.5.
    assert_eq!(lifetime("duo_copper"), 70.0);
    // `rangeChange = 16f` shifts the real range, not just base range.
    assert!((lifetime("duo_graphite") - (160.0 + 16.0 + 5.0 + 10.0) / 3.5).abs() < 1e-4);
    // `Blocks.java:3853` salvo default margin 9: (190 + 9 + 10) / 2.5.
    assert!((lifetime("salvo_copper") - 83.6).abs() < 1e-4);
    // `Blocks.java:3544` hail `limitRange(0f)` (range 235).
    assert!((lifetime("hail_graphite") - (235.0 + 0.0 + 10.0) / 3.0).abs() < 1e-4);
    // `Blocks.java:4487` breach `limitRange(12f)`: (190 + 12 + 10) / 7.5.
    assert!((lifetime("breach_beryllium") - (190.0 + 12.0 + 10.0) / 7.5).abs() < 1e-4);
    // `Blocks.java:4577` diffuse `limitRange(25f)` (range 125).
    assert!(
        (lifetime("diffuse_graphite") - (125.0 + 25.0 + 10.0) / 8.0).abs() < 1e-4,
        "diffuse graphite lifetime {}",
        lifetime("diffuse_graphite")
    );
    // `Blocks.java:5054` disperse `limitRange(16f)` + `extraRangeMargin 32`
    // (`Blocks.java:4922`): rangeChange -120 → (310 - 120 + 16 + 32 + 10) / 9.5.
    assert!(
        (lifetime("disperse_thorium") - (310.0 - 120.0 + 16.0 + 32.0 + 10.0) / 9.5).abs() < 1e-4,
        "disperse thorium lifetime {}",
        lifetime("disperse_thorium")
    );
    // `Blocks.java:5177` afflict `PowerTurret.limitRange(-55f)`: (368 - 55 + 10) / 5.
    assert!((lifetime("afflict_orb") - (368.0 - 55.0 + 10.0) / 5.0).abs() < 1e-4);
}

// ---- GAPS #3: ErekirTechTree.rebalance part (b) ----------------------

/// `ErekirTechTree.rebalance()` (`ErekirTechTree.java:37-47`) scales ammo
/// damage of turrets with non-Serpulo requirements by `0.75`; `PowerTurret`
/// is excluded by the Java `instanceof` chain.
#[test]
fn erekir_turret_ammo_rebalanced() {
    let harness = CombatHarness::new(16, 16, 7);
    let damage = |name: &str| {
        let id = harness.content().bullet_by_name(name).expect(name);
        harness.content().bullet(id).expect(name).damage
    };
    assert_eq!(damage("breach_beryllium"), 85.0 * 0.75);
    assert_eq!(damage("breach_tungsten"), 95.0 * 0.75);
    assert_eq!(damage("diffuse_graphite"), 41.0 * 0.75);
    assert_eq!(damage("titan_thorium"), 350.0 * 0.75);
    assert_eq!(damage("disperse_surge"), 65.0 * 0.75);
    assert_eq!(damage("sublimate_ozone"), 60.0 * 0.75);
    assert_eq!(damage("lustre_laser"), 210.0 * 0.75);
    assert_eq!(damage("smite_orb"), 250.0 * 0.75);
    // Serpulo turrets are untouched.
    assert_eq!(damage("duo_copper"), 9.0);
    assert_eq!(damage("salvo_copper"), 15.0);
    // `PowerTurret` (afflict/malign) is outside Java's `instanceof` chain.
    assert_eq!(damage("afflict_orb"), 180.0);
    assert_eq!(damage("malign_flak"), 70.0);
}

// ---- GAPS #5: cheating() --------------------------------------------

/// Installs a `CampaignRuntime` whose `team` rule has `cheat` set
/// (`TeamComp.cheating()` reads `team.rules().cheat`).
fn install_cheat_rules(h: &mut CombatHarness, team: u8) {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    // `BuildHarness::place` assigns `rules.default_team` to new buildings.
    h.build.rules.default_team = team;
    let campaign = Campaign::from_registry(h.content(), &EmptyNeighborhood);
    let mut rules = Rules {
        default_team: team,
        ..Rules::default()
    };
    rules.team_rule_mut(team).cheat = true;
    h.build
        .world
        .insert_resource(CampaignRuntime::new(PlaySession::new(rules), campaign));
}

/// `Turret.java:715`: a cheating team's turret reports ammo even below
/// `ammoPerShot`, and `useAmmo` (`:685`) does not consume it.
#[test]
fn cheating_gives_infinite_ammo() {
    let mut h = CombatHarness::new(32, 32, 7);
    let turret = h
        .spawn_test_turret("breach", 60.0, 60.0, 1)
        .expect("breach");
    let tungsten = h.content().item_id("beryllium").expect("beryllium");
    let bullet = h
        .content()
        .bullet_by_name("breach_beryllium")
        .expect("ammo");
    {
        let state = state_of_mut(&mut h.build.world, turret).expect("state");
        state.ammo.push(AmmoEntry {
            item: tungsten,
            bullet,
            amount: 1, // < ammoPerShot (2)
        });
        state.total_ammo = 1;
    }
    assert!(!has_ammo(&h.build.world, turret), "1 < ammoPerShot");
    install_cheat_rules(&mut h, 1);
    assert!(has_ammo(&h.build.world, turret), "cheat ignores the stack");
    // `useAmmo` leaves the stack untouched.
    let mut state = h
        .build
        .world
        .entity_mut(turret)
        .take::<TurretState>()
        .expect("state");
    let used = use_ammo(&mut state, &mut h.build.world, turret);
    assert_eq!(used, Some(bullet));
    assert_eq!(state.total_ammo, 1, "cheat never consumes ammo");
    assert_eq!(state.ammo[0].amount, 1);
    h.build.world.entity_mut(turret).insert(state);
}

/// `ItemTurret.java:103-110`: a cheating block placed without ammo gets its
/// first ammo type in `onProximityAdded`.
#[test]
fn cheating_item_turret_auto_fills_on_placement() {
    let mut h = CombatHarness::new(32, 32, 7);
    install_cheat_rules(&mut h, 1);
    let duo = h.content().block_id("duo").expect("duo");
    assert!(h.place(4, 4, duo, 1, true), "place duo");
    let turret = h.build_at(4, 4).expect("turret building");
    let state = state_of(&h.build.world, turret).expect("state");
    assert_eq!(state.total_ammo, 2, "first ammo type auto-filled");
    assert_eq!(state.ammo.len(), 1);
    assert_eq!(
        state.ammo[0].item,
        h.content().item_id("copper").expect("copper")
    );
}

/// `Turret.java:677-681`: the heat efficiency multiplier is floored at 1
/// only for a cheating team, capped by `maxHeatEfficiency`.
#[test]
fn cheating_bypasses_heat_efficiency_floor() {
    let mut h = CombatHarness::new(32, 32, 7);
    let turret = h
        .spawn_test_turret("afflict", 60.0, 60.0, 1)
        .expect("afflict");
    let mut state = h
        .build
        .world
        .entity_mut(turret)
        .take::<TurretState>()
        .expect("state");
    state.heat_req = 0.0;
    let config = state.config.clone();
    assert_eq!(
        update_efficiency_multiplier(&h.build.world, turret, 1.0, &state, &config),
        0.0,
        "no heat and no cheat -> efficiency 0"
    );
    h.build.world.entity_mut(turret).insert(state);
    install_cheat_rules(&mut h, 1);
    let state = state_of(&h.build.world, turret).expect("state");
    assert_eq!(
        update_efficiency_multiplier(&h.build.world, turret, 1.0, state, &config),
        1.0,
        "cheat floors the heat ratio at 1 (capped by maxHeatEfficiency 1)"
    );
}

// ---- GAPS #7: LiquidTurret fire extinguishing -------------------------

/// `LiquidTurretBuild.findTarget` (`LiquidTurret.java:91-119`): a liquid
/// turret whose current liquid can extinguish targets the nearest fire in
/// range instead of running the standard targeting pass.
#[test]
fn liquid_turret_targets_fires() {
    let mut h = CombatHarness::new(32, 32, 7);
    let water = h.content().liquid_id("water").expect("water");
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = h.spawn_test_turret("wave", tx, ty, 1).expect("wave");
    {
        let mut liquids = h
            .build
            .world
            .get_mut::<LiquidModule>(turret)
            .expect("liquids");
        liquids.liquids[water.index()] = 100.0;
        liquids.current_liquid = water;
    }
    // A fire on the turret's right, inside `range` (110).
    let fire = crate::combat::fires::create(&mut h.build.world, 10, 8, &mut h.rng).expect("fire");
    // A fire on an enemy block must be ignored (`other.team() == team` gate).
    let wall = h.content().block_id("copper-wall").expect("wall");
    assert!(h.place(3, 8, wall, 0, true));
    let blocked_fire =
        crate::combat::fires::create(&mut h.build.world, 3, 8, &mut h.rng).expect("fire");
    state_of_mut(&mut h.build.world, turret)
        .expect("state")
        .target_timer = 0.0;
    h.update_turrets_only();
    assert_eq!(
        h.turret_state(turret).expect("state").target,
        Some(fire),
        "nearest own-team fire is targeted"
    );
    let _ = blocked_fire;
}

/// A non-extinguishing liquid (oil) never takes the fire path
/// (`liquids.current().canExtinguish()`).
#[test]
fn liquid_turret_ignores_fire_with_flammable_ammo() {
    let mut h = CombatHarness::new(32, 32, 7);
    let oil = h.content().liquid_id("oil").expect("oil");
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = h.spawn_test_turret("wave", tx, ty, 1).expect("wave");
    {
        let mut liquids = h
            .build
            .world
            .get_mut::<LiquidModule>(turret)
            .expect("liquids");
        liquids.liquids[oil.index()] = 100.0;
        liquids.current_liquid = oil;
    }
    let _fire = crate::combat::fires::create(&mut h.build.world, 10, 8, &mut h.rng);
    state_of_mut(&mut h.build.world, turret)
        .expect("state")
        .target_timer = 0.0;
    h.update_turrets_only();
    assert_eq!(
        h.turret_state(turret).expect("state").target,
        None,
        "oil cannot extinguish, so the fire is not a target"
    );
}

// ---- GAPS #8: TractorBeamTurret --------------------------------------

/// `TractorBeamTurret.java:127-130`: the beam uses
/// `damageContinuousPierce` — shields absorb first, the death path runs —
/// and `targetGround = false` keeps grounded units safe.
#[test]
fn tractor_beam_damages_through_shields_and_ignores_ground() {
    use crate::entities::comp::unit::comp::{PhysicsComp, ShieldComp};

    let mut h = CombatHarness::new(64, 64, 7);
    let (tx, ty) = CombatHarness::tile_center(8, 8);
    let turret = h
        .spawn_test_turret("parallax", tx, ty, 1)
        .expect("parallax");
    // Default turret rotation is 90° (+y): place the air target straight up.
    let air = h.spawn_test_unit(tx, ty + 100.0, 2, Vec::new());
    h.build.world.entity_mut(air).insert(PhysicsComp {
        flying: true,
        ..PhysicsComp::default()
    });
    h.build.world.entity_mut(air).insert(ShieldComp {
        shield: 2.0,
        shield_alpha: 0.0,
    });
    // A grounded unit at the same distance must be ignored.
    let ground = h.spawn_test_unit(tx, ty + 60.0, 2, Vec::new());
    {
        let state = state_of_mut(&mut h.build.world, turret).expect("state");
        state.retarget_timer = 0.0;
        state.target_timer = 0.0;
        state.rotation = 90.0;
    }
    for _ in 0..10 {
        h.update_turrets_only();
    }
    let shield = h.build.world.get::<ShieldComp>(air).expect("shield").shield;
    let air_health = h.build.world.get::<Health>(air).expect("health").health;
    assert!(shield < 2.0, "the shield absorbed beam damage: {shield}");
    assert!(
        air_health < 1_000.0,
        "the beam also reaches health after the shield: {air_health}"
    );
    assert_eq!(
        h.build.world.get::<Health>(ground).expect("health").health,
        1_000.0,
        "grounded units are outside `targetGround = false`"
    );
}

/// PAR-0528: `BuildTurret.java:156-176` — with no queued plan the tower
/// scans the team rebuild queue, takes the first in-range legal entry,
/// rotates it to the tail (`lastPlan` is recorded) and drives the
/// `ConstructBuild` at that tile.
#[test]
fn build_turret_scans_the_team_rebuild_queue() {
    use crate::ecs::TeamId;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::teams::BlockPlan;
    use crate::game::universe::Campaign;

    let mut harness = CombatHarness::new(48, 16, 7);
    let wall = harness.content().block_id("copper-wall").expect("wall");
    assert!(harness.place(10, 8, wall, 0, false));
    let construct = harness.build_at(10, 8).expect("construct entity");
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let _turret = harness
        .spawn_test_turret("build-tower", tx, ty, 1)
        .expect("build tower");
    // The team rebuild queue is a `CampaignRuntime` field
    // (`team.data().plans`).
    let campaign = Campaign::from_registry(harness.build.content(), &EmptyNeighborhood);
    let session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    let mut runtime = CampaignRuntime::new(session, campaign);
    runtime
        .session
        .teams
        .get(TeamId(1))
        .plans
        .push_back(BlockPlan {
            x: 10,
            y: 8,
            rotation: 0,
            block: wall,
            config: None,
            removed: false,
        });
    runtime
        .session
        .teams
        .get(TeamId(1))
        .plans
        .push_back(BlockPlan {
            x: 30,
            y: 8,
            rotation: 0,
            block: wall,
            config: None,
            removed: false,
        });
    harness.build.world.insert_resource(runtime);

    for _ in 0..12 {
        harness.tick();
    }
    let progress = harness
        .build
        .world
        .get::<ConstructState>(construct)
        .expect("construct state")
        .progress;
    assert!(progress > 0.0, "team-queue plan drove the construction");
    // `blocks.addLast(blocks.removeIndex(i))`: the taken plan moved to the
    // tail, the out-of-range one is now first — proof the scan selected
    // `(10, 8)` rather than leaving the queue in insertion order.
    let queue = &harness
        .build
        .world
        .get_resource::<CampaignRuntime>()
        .expect("runtime")
        .session
        .teams
        .get_or_null(TeamId(1))
        .expect("team data")
        .plans;
    assert_eq!(queue.len(), 2);
    assert_eq!((queue[0].x, queue[0].y), (30, 8));
    assert_eq!((queue[1].x, queue[1].y), (10, 8));
}
