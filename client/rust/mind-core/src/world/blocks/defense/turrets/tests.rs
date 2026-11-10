// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the turret state machine, ammo and shooting pipeline.
//! Upstream: none (port-only; turret tests)

use super::types::{turret as state_of, turret_mut as state_of_mut};
use super::*;
use crate::combat::harness::CombatHarness;
use crate::content::registries::sound_meta::SoundId;
use crate::world::construct::ConstructState;
use crate::world::modules::{LiquidModule, PowerModule};
use crate::world::plan::BuildPlan;
use bevy_ecs::entity::Entity;

/// Sets a test unit's `elevation` plus the flying-type mirror
/// (`UnitType.java:566`: `unit.elevation = flying ? 1f : 0`), so targeting
/// fixtures can occupy Java's elevation bands (`UnitComp.java:80-86`).
pub(super) fn set_unit_elevation(
    harness: &mut CombatHarness,
    unit: Entity,
    elevation: f32,
    physics_flying: bool,
) {
    use crate::entities::comp::unit::comp::{PhysicsComp, UnitCore};
    let world = &mut harness.build.world;
    world.entity_mut(unit).insert(UnitCore {
        elevation,
        ..UnitCore::new(0.0)
    });
    world.entity_mut(unit).insert(PhysicsComp {
        flying: physics_flying,
        ..PhysicsComp::default()
    });
}

fn turret_at(harness: &mut CombatHarness, name: &str) -> (Entity, Entity) {
    let wall = harness.content().block_id("copper-wall").expect("wall");
    // Target at tile 10,8; turret at tile 4,8.
    assert!(harness.place(10, 8, wall, 0, true));
    let target = harness.build_at(10, 8).expect("target");
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = harness
        .spawn_test_turret(name, tx, ty, 1)
        .unwrap_or_else(|| panic!("turret `{name}`"));
    (turret, target)
}

#[test]
fn duo_graphite_range_is_176() {
    let mut harness = CombatHarness::new(32, 16, 7);
    let (turret, _) = turret_at(&mut harness, "duo");
    assert_eq!(
        turret_range(&harness.build.world, turret),
        160.0,
        "base range"
    );
    let graphite = harness.content().item_id("graphite").expect("graphite");
    handle_item(&mut harness.build.world, turret, graphite);
    assert_eq!(
        turret_range(&harness.build.world, turret),
        176.0,
        "graphite rangeChange 16"
    );
    let silicon = harness.content().item_id("silicon").expect("silicon");
    handle_item(&mut harness.build.world, turret, silicon);
    // `peekAmmo()` is the last stack; silicon has no rangeChange.
    assert_eq!(turret_range(&harness.build.world, turret), 160.0);
}

#[test]
fn item_ammo_acceptance_and_merge() {
    let mut harness = CombatHarness::new(32, 16, 7);
    let (turret, _) = turret_at(&mut harness, "test-item");
    let copper = harness.content().item_id("copper").expect("copper");
    let lead = harness.content().item_id("lead").expect("lead");
    assert!(accept_item(&harness.build.world, turret, copper));
    assert!(!accept_item(&harness.build.world, turret, lead));
    handle_item(&mut harness.build.world, turret, copper);
    handle_item(&mut harness.build.world, turret, copper);
    let state = state_of(&harness.build.world, turret).expect("state");
    assert_eq!(state.ammo.len(), 1, "same item merges into one entry");
    assert_eq!(state.ammo[0].amount, 4, "duo copper ammoMultiplier 2 x2");
    assert!(has_ammo(&harness.build.world, turret));
}

#[test]
fn ammo_consumption_decrements_total() {
    let mut harness = CombatHarness::new(32, 16, 7);
    let (turret, target) = turret_at(&mut harness, "test-item");
    let copper = harness.content().item_id("copper").expect("copper");
    for _ in 0..5 {
        handle_item(&mut harness.build.world, turret, copper);
    }
    let before = state_of(&harness.build.world, turret).unwrap().total_ammo;
    let hp_before = harness.building_health_at(10, 8);
    for _ in 0..40 {
        harness.tick();
    }
    let state = state_of(&harness.build.world, turret).unwrap();
    assert!(state.total_ammo < before, "ammo consumed");
    assert!(state.total_shots > 0, "turret fired");
    assert!(
        harness.building_health_at(10, 8) < hp_before,
        "target damaged"
    );
    let _ = target;
}

#[test]
fn coolant_speeds_reload() {
    let mut harness = CombatHarness::new(32, 16, 7);
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = harness
        .spawn_test_turret("test-item", tx, ty, 1)
        .expect("turret");
    let water = harness.content().liquid_id("water").expect("water");
    let copper = harness.content().item_id("copper").expect("copper");
    handle_item(&mut harness.build.world, turret, copper);
    // No target: `handleReload` still accumulates (reload is independent).
    for _ in 0..10 {
        harness.tick();
    }
    let without = state_of(&harness.build.world, turret)
        .unwrap()
        .reload_counter;
    assert!(without > 0.0, "reload accumulated without a target");
    state_of_mut(&mut harness.build.world, turret)
        .unwrap()
        .reload_counter = 0.0;
    harness
        .build
        .world
        .get_mut::<LiquidModule>(turret)
        .unwrap()
        .add(water, 20.0, 60.0);
    for _ in 0..10 {
        harness.tick();
    }
    let with = state_of(&harness.build.world, turret)
        .unwrap()
        .reload_counter;
    assert!(with > without, "coolant boosted reload: {with} > {without}");
}

#[test]
fn power_gate_blocks_until_powered() {
    let mut harness = CombatHarness::new(32, 16, 7);
    let (turret, _) = turret_at(&mut harness, "test-power");
    harness
        .build
        .world
        .get_mut::<PowerModule>(turret)
        .unwrap()
        .status = 0.0;
    for _ in 0..40 {
        harness.tick();
    }
    assert_eq!(
        state_of(&harness.build.world, turret).unwrap().total_shots,
        0,
        "no shots without power"
    );
    harness
        .build
        .world
        .get_mut::<PowerModule>(turret)
        .unwrap()
        .status = 1.0;
    for _ in 0..40 {
        harness.tick();
    }
    assert!(
        state_of(&harness.build.world, turret).unwrap().total_shots > 0,
        "shoots once powered"
    );
}

#[test]
fn cone_gate_delays_first_shot_until_aligned() {
    let mut harness = CombatHarness::new(48, 16, 7);
    let wall = harness.content().block_id("copper-wall").expect("wall");
    assert!(harness.place(10, 8, wall, 0, true));
    // Turret starts at rotation 90 (up); target is to the right (angle 0).
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = harness
        .spawn_test_turret("test-item", tx, ty, 1)
        .expect("turret");
    let copper = harness.content().item_id("copper").expect("copper");
    for _ in 0..10 {
        handle_item(&mut harness.build.world, turret, copper);
    }
    // One tick: reload not full, so no shot regardless; step until aligned.
    let mut fired_tick = None;
    for tick in 1..=30 {
        harness.tick();
        if state_of(&harness.build.world, turret).unwrap().total_shots > 0 {
            fired_tick = Some(tick);
            break;
        }
    }
    assert!(fired_tick.is_some(), "turret aligned and fired");
}

#[test]
fn activation_timer_blocks_shooting() {
    let mut harness = CombatHarness::new(32, 16, 7);
    let (turret, _) = turret_at(&mut harness, "test-item");
    state_of_mut(&mut harness.build.world, turret)
        .unwrap()
        .activation_timer = 5.0;
    let copper = harness.content().item_id("copper").expect("copper");
    for _ in 0..5 {
        handle_item(&mut harness.build.world, turret, copper);
    }
    for _ in 0..3 {
        harness.tick();
    }
    assert_eq!(
        state_of(&harness.build.world, turret).unwrap().total_shots,
        0,
        "activation timer blocks fire"
    );
}

#[test]
fn payload_ammo_accept_and_consume() {
    let mut harness = CombatHarness::new(48, 16, 7);
    let wall = harness.content().block_id("copper-wall").expect("wall");
    // Target at tile 10,8; turret at tile 4,8.
    assert!(harness.place(10, 8, wall, 0, true));
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = harness
        .spawn_test_turret("test-payload", tx, ty, 1)
        .expect("payload turret");
    let payload = crate::world::PayloadRef {
        entity: None,
        content: wall.raw(),
        is_block: true,
    };
    assert!(accept_payload(&harness.build.world, turret, payload));
    for _ in 0..3 {
        handle_payload(&mut harness.build.world, turret, payload);
    }
    // A fourth payload is rejected at the capacity of 3.
    assert!(!accept_payload(&harness.build.world, turret, payload));
    let hp_before = harness.building_health_at(10, 8);
    for _ in 0..80 {
        harness.tick();
    }
    let state = state_of(&harness.build.world, turret).unwrap();
    assert!(state.total_shots > 0, "payload turret fired");
    assert!(state.total_ammo < 3, "payload ammo consumed");
    assert!(
        harness.building_health_at(10, 8) < hp_before,
        "payload bullet damaged target"
    );
}

#[test]
fn build_turret_advances_construction() {
    let mut harness = CombatHarness::new(48, 16, 7);
    let wall = harness.content().block_id("copper-wall").expect("wall");
    // Non-instant placement creates a plan-07 construction at the tile.
    assert!(harness.place(10, 8, wall, 0, false));
    let construct = harness.build_at(10, 8).expect("construct entity");
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = harness
        .spawn_test_turret("build-tower", tx, ty, 1)
        .expect("build tower");
    assert!(add_build_plan(
        &mut harness.build.world,
        turret,
        BuildPlan::place(10, 8, 0, wall)
    ));
    for _ in 0..20 {
        harness.tick();
    }
    let progress = harness
        .build
        .world
        .get::<ConstructState>(construct)
        .expect("construct state")
        .progress;
    assert!(progress > 0.0, "build turret advanced the construct");
    let state = state_of(&harness.build.world, turret).unwrap();
    assert!(state.build_warmup > 0.0, "build warmup ramped");
}

/// L06 stretch: a heal-suppressed build tower builds at efficiency 0
/// (`checkSuppression`, BuildTurret.java:132-134; `update_build` routes its
/// efficiency through `build_turret_efficiency`).
#[test]
fn suppressed_build_turret_has_zero_efficiency() {
    let mut harness = CombatHarness::new(48, 16, 7);
    let tower = harness.content().block_id("build-tower").expect("tower");
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = harness
        .spawn_test_turret("build-tower", tx, ty, 1)
        .expect("build tower");
    // `checkSuppression` reads the building's block (`suppressable`) and
    // `healSuppressionTime`.
    harness
        .build
        .world
        .entity_mut(turret)
        .insert(crate::entities::comp::Building::new(
            crate::world::TilePos::new(4, 8),
            tower,
            0,
        ));
    assert_eq!(
        build_turret_efficiency(&harness.build.world, turret, 1.0),
        1.0
    );
    crate::world::blocks::defense::projectors::apply_suppression(
        &mut harness.build.world,
        2,
        tx,
        ty,
        40.0,
        600.0,
    );
    assert_eq!(
        build_turret_efficiency(&harness.build.world, turret, 1.0),
        0.0,
        "a suppressed build turret builds nothing"
    );
    // Own-team suppression never applies (`Damage.applySuppression`).
    let mut harness = CombatHarness::new(48, 16, 7);
    let turret = harness
        .spawn_test_turret("build-tower", tx, ty, 1)
        .expect("build tower");
    harness
        .build
        .world
        .entity_mut(turret)
        .insert(crate::entities::comp::Building::new(
            crate::world::TilePos::new(4, 8),
            tower,
            0,
        ));
    crate::world::blocks::defense::projectors::apply_suppression(
        &mut harness.build.world,
        1,
        tx,
        ty,
        40.0,
        600.0,
    );
    assert_eq!(
        build_turret_efficiency(&harness.build.world, turret, 1.0),
        1.0
    );
}

#[test]
fn turret_predict_flags_match_blocks_java() {
    let harness = CombatHarness::new(8, 8, 1);
    let cfg = |name: &str| {
        config_for(harness.content(), name, harness.names_map())
            .unwrap_or_else(|| panic!("config `{name}`"))
    };
    // `Turret.java:84,86,111`: class defaults (accurateDelay, moveWhileCharging,
    // predictTarget all true unless a block overrides them).
    let duo = cfg("duo");
    assert!(duo.predict_target);
    assert!(duo.accurate_delay);
    assert!(duo.move_while_charging);
    // `Blocks.java:3594-3595`: lancer sets moveWhileCharging/accurateDelay false.
    let lancer = cfg("lancer");
    assert!(!lancer.accurate_delay);
    assert!(!lancer.move_while_charging);
    assert!(lancer.predict_target);
    // `Blocks.java:5248`: scathe sets predictTarget false.
    let scathe = cfg("scathe");
    assert!(!scathe.predict_target);
    assert!(scathe.accurate_delay);
    assert!(scathe.move_while_charging);
}

#[test]
fn turret_sound_table_matches_blocks_java() {
    let harness = CombatHarness::new(8, 8, 1);
    let cfg = |name: &str| {
        config_for(harness.content(), name, harness.names_map())
            .unwrap_or_else(|| panic!("config `{name}`"))
    };
    // `Blocks.java:3540`: hail carries shootArtillerySmall at volume 1.
    let hail = cfg("hail");
    assert_eq!(hail.shoot_sound, SoundId::SHOOT_ARTILLERY_SMALL);
    assert_eq!(hail.shoot_sound_volume, 1.0);
    // `Blocks.java:3952-3953`: fuse volume 0.9.
    assert_eq!(cfg("fuse").shoot_sound_volume, 0.9);
    // `Blocks.java:3596-3598`: lancer chargeSound.
    assert_eq!(cfg("lancer").charge_sound, SoundId::CHARGE_LANCER);
    assert_eq!(cfg("lancer").shoot_sound, SoundId::SHOOT_LANCER);
    // `Blocks.java`: wave/tsunami are shoot-soundless LiquidTurrets with a
    // `loopSpray` loop.
    let wave = cfg("wave");
    assert!(wave.shoot_sound.is_none_or_unset());
    assert_eq!(wave.loop_sound, SoundId::LOOP_SPRAY);
    assert_eq!(wave.loop_sound_volume, 0.5);
    // `Blocks.java:4352-4354`: meltdown loop volume 2.0.
    assert_eq!(cfg("meltdown").loop_sound, SoundId::BEAM_MELTDOWN);
    assert_eq!(cfg("meltdown").loop_sound_volume, 2.0);
    // `Blocks.java:5222-5224`: lustre loops beamLustre at volume 1.0.
    assert_eq!(cfg("lustre").loop_sound, SoundId::BEAM_LUSTRE);
    assert_eq!(cfg("lustre").loop_sound_volume, 1.0);
    // `PointDefenseTurret.java:30`: segment has its own intercept sound.
    assert_eq!(cfg("segment").shoot_sound, SoundId::SHOOT_SEGMENT);
    // `Turret.java:127-138` class defaults survive where Blocks.java does
    // not override.
    let duo = cfg("duo");
    assert_eq!(duo.shoot_sound, SoundId::SHOOT_DUO);
    assert_eq!(duo.shoot_sound_volume, 1.0);
    assert!(duo.charge_sound.is_none_or_unset());
    assert_eq!(
        (duo.sound_pitch_min, duo.sound_pitch_max),
        (0.9, 1.1),
        "Turret.java:127-138 pitch window"
    );
}

#[test]
fn hail_shot_emits_its_own_sound() {
    use crate::audio::{AudioEvent, SharedAudioLog};
    let mut harness = CombatHarness::new(48, 16, 7);
    let log = SharedAudioLog::new();
    harness.build.set_audio_sink(log.clone());
    let wall = harness.content().block_id("copper-wall").expect("wall");
    assert!(harness.place(10, 8, wall, 0, true));
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = harness
        .spawn_test_turret("hail", tx, ty, 1)
        .unwrap_or_else(|| panic!("hail"));
    let graphite = harness.content().item_id("graphite").expect("graphite");
    for _ in 0..10 {
        handle_item(&mut harness.build.world, turret, graphite);
    }
    for _ in 0..180 {
        harness.tick();
    }
    assert!(
        state_of(&harness.build.world, turret).unwrap().total_shots > 0,
        "hail fired"
    );
    let mut matched = 0;
    for event in log.events() {
        if let AudioEvent::At { sound, pitch, .. } = &event.event
            && *sound == SoundId::SHOOT_ARTILLERY_SMALL
        {
            assert!((0.9..=1.1).contains(pitch), "pitch {pitch}");
            matched += 1;
        }
    }
    assert!(matched > 0, "hail emitted its own shoot sound");
}

/// Runs the harness to the first spawned bullet, then returns the per-tick
/// spawn counts for the next `window` ticks (index 0 is the first shot's
/// tick). `bullets_created` is monotonic, so hits/expiry cannot hide a
/// spawn.
fn volley_spawn_counts(harness: &mut CombatHarness, window: usize) -> Vec<u64> {
    let mut first = None;
    for _ in 0..240 {
        let before = harness.bullets_created;
        harness.tick();
        if harness.bullets_created > before {
            first = Some(harness.bullets_created - before);
            break;
        }
    }
    let first = first.expect("turret fired");
    let mut counts = vec![first];
    for _ in 0..window {
        let before = harness.bullets_created;
        harness.tick();
        counts.push(harness.bullets_created - before);
    }
    counts
}

/// PAR-0451 (`Turret.java:751-830`): `shoot()` defers every pattern shot
/// with `delay > 0` through `Time.run`, so a multi-shot pattern fires one
/// bullet per delayed tick instead of collapsing the burst onto the reload
/// tick.
#[test]
fn multi_shot_patterns_fire_on_their_delayed_ticks() {
    // `Blocks.java:3846-3847`: salvo `shots = 4; shotDelay = 3` => T, +3,
    // +6, +9. `Blocks.java:3733-3734`: swarmer `ShootBarrel` `shots = 4;
    // shotDelay = 5` => T, +5, +10, +15.
    for (name, item, offsets) in [
        ("salvo", "copper", [0usize, 3, 6, 9]),
        ("swarmer", "blast-compound", [0, 5, 10, 15]),
    ] {
        let mut harness = CombatHarness::new(48, 16, 7);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 8, wall, 0, true));
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = harness
            .spawn_test_turret(name, tx, ty, 1)
            .unwrap_or_else(|| panic!("turret `{name}`"));
        let item = harness.content().item_id(item).expect(item);
        for _ in 0..20 {
            handle_item(&mut harness.build.world, turret, item);
        }
        let counts = volley_spawn_counts(&mut harness, offsets[3] + 1);
        let ticks: Vec<usize> = counts
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .map(|(tick, _)| tick)
            .collect();
        assert_eq!(
            ticks, offsets,
            "`{name}` fires one shot per delayed tick, not a burst"
        );
        assert_eq!(counts.iter().sum::<u64>(), 4, "`{name}` 4 shots total");
    }
}

/// PAR-0451: scatter's 2-shot pattern (`Blocks.java:3420-3421`
/// `shotDelay = 5; shots = 2`) spreads over T and T+5. Scatter is anti-air
/// (`targetGround = false`, `Turret.java:645`), so the harness supplies an
/// airborne stand-in target (`!isGrounded()`, `UnitComp.java:80-82`); the
/// assertion only covers spawn ticks.
#[test]
fn scatter_fires_one_shot_per_delayed_tick() {
    let mut harness = CombatHarness::new(48, 16, 7);
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = harness
        .spawn_test_turret("scatter", tx, ty, 1)
        .expect("scatter");
    let scrap = harness.content().item_id("scrap").expect("scrap");
    for _ in 0..10 {
        handle_item(&mut harness.build.world, turret, scrap);
    }
    // A grounded stand-in would be rejected by the air-only branch; this
    // one hovers at elevation 1, like a flying unit type
    // (`UnitType.java:566`).
    let target = harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
    set_unit_elevation(&mut harness, target, 1.0, true);
    let counts = volley_spawn_counts(&mut harness, 6);
    let ticks: Vec<usize> = counts
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(tick, _)| tick)
        .collect();
    assert_eq!(ticks, vec![0, 5], "scatter T and T+5, not 2 at T");
    assert_eq!(counts.iter().sum::<u64>(), 2);
}

/// PAR-0451 (`Turret.java:718-720,742,751-830`): `charging()` is
/// `queuedBullets > 0 && shoot.firstShotDelay > 0`, so lancer's 40-tick
/// `firstShotDelay` queues its only shot, ramps `charge` at 1/40 per tick,
/// freezes rotation (`moveWhileCharging = false`, `Blocks.java:3594`) and
/// gates `updateShooting` until the shot fires.
#[test]
fn lancer_charges_before_its_delayed_shot() {
    let mut harness = CombatHarness::new(48, 16, 7);
    let wall = harness.content().block_id("copper-wall").expect("wall");
    assert!(harness.place(10, 8, wall, 0, true));
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = harness
        .spawn_test_turret("lancer", tx, ty, 1)
        .expect("lancer");
    {
        let state = state_of_mut(&mut harness.build.world, turret).unwrap();
        // `reload = 80` (`Blocks.java:3586`): start full so the first tick
        // queues the charged shot; aim already on the wall (angle 0) and a
        // zeroed retarget timer acquire it immediately.
        state.reload_counter = 80.0;
        state.target_timer = 0.0;
        state.rotation = 0.0;
    }
    // Tick 1 queues the shot: nothing spawns, and `charge` is still 0 (it is
    // updated before `updateShooting`).
    harness.tick();
    let state = harness.turret_state(turret).unwrap();
    assert_eq!(harness.bullets_created, 0, "no bullet on the queue tick");
    assert_eq!(state.total_shots, 0);
    assert_eq!(state.charge, 0.0);
    // Freeze probe: aim the turret away; `shouldTurn()` is false while
    // charging, so the rotation must not follow the target.
    state_of_mut(&mut harness.build.world, turret)
        .unwrap()
        .rotation = 90.0;
    let mut charge_mid = 0.0f32;
    for k in 2..=40 {
        harness.tick();
        let state = harness.turret_state(turret).unwrap();
        assert_eq!(harness.bullets_created, 0, "no bullet on charge tick {k}");
        assert_eq!(state.rotation, 90.0, "rotation frozen on charge tick {k}");
        if k == 21 {
            charge_mid = state.charge;
        }
    }
    let state = harness.turret_state(turret).unwrap();
    assert!(
        (charge_mid - 0.5).abs() < 0.01,
        "charge ramps 1/40 per tick, got {charge_mid} at tick 21"
    );
    assert!(
        state.reload_counter > 30.0,
        "reload continues while charging (`reloadWhileCharging`): {}",
        state.reload_counter
    );
    // The 40th tick after queueing fires the single charged shot.
    harness.tick();
    let state = harness.turret_state(turret).unwrap();
    assert_eq!(harness.bullets_created, 1, "exactly one charged shot");
    assert_eq!(state.total_shots, 1);
    assert_eq!(state.charge, 0.0, "charge resets after firing");
}
