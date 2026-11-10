// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! I03 T4/T5 turret control and radar conformance.
//! Upstream: core/src/mindustry/logic/LExecutor.java.

use super::units::entity_pos;
use super::world_effects::processor_obj;
use super::*;

// ---- I03 T4: logic-controlled turrets (PAR-0223) ----

/// `control shoot` aims the turret at the logic point and fires.
#[test]
fn control_shoot_aims_duo() {
    let mut sim = campaign_sim();
    let duo = place_team1(&mut sim, "duo", 18, 18);
    feed_turret(&mut sim, duo, "copper", 10);
    place_world_processor(
        &mut sim,
        10,
        10,
        "getblock building d 18 18\ncontrol shoot d 23 18 1\nstop\n",
    );
    tick_n(&mut sim, 40);
    let state = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .expect("turret");
    let cone = state.config.shoot_cone;
    let rotation = state.rotation;
    let bullets = sim.unit_runtime().expect("runtime").bullets.len();
    assert!(
        crate::weapons::angle_dist(rotation, 0.0) < cone,
        "aims east at the logic point: {rotation}"
    );
    assert!(bullets > 0, "fired at the logic point");
    // `stop` + the 120-tick cooldown ends logic control: no more shots.
    let bytes = crate::logic::blocks::logic_block::compress("stop\n", &[]);
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    tick_n(&mut sim, 100);
    let after_cooldown = sim.unit_runtime().expect("runtime").bullets.len();
    tick_n(&mut sim, 200);
    let later = sim.unit_runtime().expect("runtime").bullets.len();
    assert!(
        later <= after_cooldown,
        "no new bullets after the logic cooldown ({after_cooldown} -> {later})"
    );
    assert_events_drained(&sim);
}

/// `control shootp` follows a unit target through `targetPosition`.
#[test]
fn control_shootp_tracks_unit() {
    let mut sim = campaign_sim();
    let duo = place_team1(&mut sim, "duo", 18, 18);
    feed_turret(&mut sim, duo, "copper", 10);
    // A team-2 core lets the target dagger spawn under the unit cap.
    let core = sim.world_block_id("core-shard").expect("core");
    sim.command(crate::determinism::SimCommand::Place {
        x: 20,
        y: 20,
        block: core,
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("enemy core");
    let target = sim
        .spawn_live_unit("dagger", 2, 22.0 * 8.0, 18.0 * 8.0, 0.0)
        .expect("dagger");
    place_world_processor(
        &mut sim,
        10,
        10,
        "getblock building d 18 18\nfetch unit u @crux 0 @dagger\ncontrol shootp d u 1\nstop\n",
    );
    tick_n(&mut sim, 10);
    let state = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .expect("turret");
    let (tx, ty) = entity_pos(&sim, target);
    let (aim_x, aim_y) = state.target_pos;
    let (dx, dy) = (aim_x - tx, aim_y - ty);
    assert!(
        dx * dx + dy * dy <= 6.0 * 6.0,
        "aim follows the target ({aim_x},{aim_y}) vs ({tx},{ty})"
    );
    assert_events_drained(&sim);
}

// ---- I03 T5: radar/uradar correctness (PAR-0222) ----

fn set_health(sim: &mut Sim, e: Entity, health: f32) {
    if let Some(mut h) = sim.ecs.0.get_mut::<crate::entities::comp::Health>(e) {
        h.health = health;
    }
}

fn enemy_core(sim: &mut Sim) {
    let core = sim.world_block_id("core-shard").expect("core");
    sim.command(crate::determinism::SimCommand::Place {
        x: 4,
        y: 4,
        block: core,
        rotation: 0,
        team: 2,
        player: None,
    })
    .expect("enemy core");
}

/// `radar ... health <src> 1` picks the max-health target; `0` the min
/// (`RadarSort` argmax with `sortDir = order ? 1 : -1`).
#[test]
fn radar_health_order() {
    let mut sim = campaign_sim();
    place_team1(&mut sim, "core-shard", 20, 20);
    let duo = place_team1(&mut sim, "duo", 18, 18);
    enemy_core(&mut sim);
    let low = sim
        .spawn_live_unit("dagger", 2, 24.0 * 8.0, 18.0 * 8.0, 0.0)
        .expect("dagger");
    let high = sim
        .spawn_live_unit("dagger", 2, 26.0 * 8.0, 18.0 * 8.0, 0.0)
        .expect("dagger");
    set_health(&mut sim, low, 50.0);
    set_health(&mut sim, high, 100.0);
    place_world_processor(
        &mut sim,
        10,
        10,
        "getblock building d 18 18\nradar enemy any any health d 1 hi\nradar enemy any any health d 0 lo\nstop\n",
    );
    tick_n(&mut sim, 5);
    let _ = duo;
    assert_eq!(
        processor_obj(&sim, 10, 10, "hi"),
        Some(LogicObject::Unit(high)),
        "order 1 takes the max health"
    );
    assert_eq!(
        processor_obj(&sim, 10, 10, "lo"),
        Some(LogicObject::Unit(low)),
        "order 0 takes the min health"
    );
    assert_events_drained(&sim);
}

/// The radar source's range filters candidates (a micro processor's 10-tile
/// range drops an out-of-range unit).
#[test]
fn radar_range() {
    let mut sim = campaign_sim();
    place_team1(&mut sim, "core-shard", 20, 20);
    place_team1(&mut sim, "micro-processor", 12, 12);
    enemy_core(&mut sim);
    let near = sim
        .spawn_live_unit("dagger", 2, 150.0, 96.0, 0.0)
        .expect("dagger");
    let far = sim
        .spawn_live_unit("dagger", 2, 220.0, 96.0, 0.0)
        .expect("dagger");
    // `getblock` is a privileged statement, so the world processor runs it and
    // passes the micro processor as the radar source.
    place_world_processor(
        &mut sim,
        10,
        10,
        "getblock building micro 12 12\nradar enemy any any distance micro 1 out\nstop\n",
    );
    tick_n(&mut sim, 5);
    assert_eq!(
        processor_obj(&sim, 10, 10, "out"),
        Some(LogicObject::Unit(near)),
        "only the in-range unit is returned"
    );
    let _ = far;
    assert_events_drained(&sim);
}

/// A building radar source refreshes on the fixed 30-tick interval.
#[test]
fn radar_cache_holds_29_ticks() {
    let mut sim = campaign_sim();
    place_team1(&mut sim, "core-shard", 20, 20);
    let duo = place_team1(&mut sim, "duo", 18, 18);
    enemy_core(&mut sim);
    let low = sim
        .spawn_live_unit("dagger", 2, 24.0 * 8.0, 18.0 * 8.0, 0.0)
        .expect("dagger");
    set_health(&mut sim, low, 50.0);
    place_world_processor(
        &mut sim,
        10,
        10,
        "getblock building d 18 18\nradar enemy any any health d 1 out\n",
    );
    tick_n(&mut sim, 5);
    assert_eq!(
        processor_obj(&sim, 10, 10, "out"),
        Some(LogicObject::Unit(low))
    );
    // A new, healthier target appears; the cached result holds until the
    // 30-tick interval elapses.
    let high = sim
        .spawn_live_unit("dagger", 2, 26.0 * 8.0, 18.0 * 8.0, 0.0)
        .expect("dagger");
    set_health(&mut sim, high, 200.0);
    tick_n(&mut sim, 25);
    assert_eq!(
        processor_obj(&sim, 10, 10, "out"),
        Some(LogicObject::Unit(low)),
        "still cached before 30 ticks"
    );
    tick_n(&mut sim, 15);
    assert_eq!(
        processor_obj(&sim, 10, 10, "out"),
        Some(LogicObject::Unit(high)),
        "refreshed after the interval"
    );
    let _ = duo;
    assert_events_drained(&sim);
}

/// `attacker` filters to units with weapons (`!disarmed`).
#[test]
fn radar_attacker_filter() {
    let mut sim = campaign_sim();
    place_team1(&mut sim, "core-shard", 20, 20);
    place_team1(&mut sim, "duo", 18, 18);
    enemy_core(&mut sim);
    // `assembly-drone` has no weapons in Java (`weapons.is_empty()`).
    let unarmed = sim
        .spawn_live_unit("assembly-drone", 2, 24.0 * 8.0, 18.0 * 8.0, 0.0)
        .expect("assembly-drone");
    let armed = sim
        .spawn_live_unit("dagger", 2, 26.0 * 8.0, 18.0 * 8.0, 0.0)
        .expect("dagger");
    // The unarmed unit is healthier: without the filter, health-order 1 would
    // pick it.
    set_health(&mut sim, unarmed, 300.0);
    set_health(&mut sim, armed, 100.0);
    place_world_processor(
        &mut sim,
        10,
        10,
        "getblock building d 18 18\nradar enemy any attacker health d 1 out\nstop\n",
    );
    tick_n(&mut sim, 5);
    assert_eq!(
        processor_obj(&sim, 10, 10, "out"),
        Some(LogicObject::Unit(armed)),
        "only the weaponed unit matches attacker"
    );
    assert_events_drained(&sim);
}
