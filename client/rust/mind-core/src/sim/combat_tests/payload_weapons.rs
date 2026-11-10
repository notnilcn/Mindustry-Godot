// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Payload pickup/drop, unit weapon and combat-FX regressions.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;

/// PAR-0138: a player `mega` picks up an adjacent friendly dagger and drops it
/// elsewhere (`requestUnitPayload` -> `dropLastPayload`).
#[test]
fn mega_picks_up_and_drops_dagger() {
    use crate::entities::comp::unit::comp::{CarriedPayload, UnitPayloads};
    let mut sim = live_sim(48);
    let mega = sim
        .spawn_player_unit("mega", 1, 100.0, 100.0, 0.0)
        .expect("mega");
    let dagger_type = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_id("dagger")
        .expect("dagger id");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 110.0, 100.0, 0.0)
        .expect("dagger");
    let seq = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    assert!(
        sim.apply_payload_command(0, 0.0, 0.0, Some(seq)),
        "the dagger is picked up"
    );
    assert!(
        sim.ecs.0.get_entity(dagger).is_err(),
        "the carried dagger left the world"
    );
    let stack = sim.ecs.0.get::<UnitPayloads>(mega).expect("payload stack");
    assert_eq!(stack.stack.len(), 1, "one carried payload");
    assert!(matches!(stack.stack[0], CarriedPayload::Unit { .. }));

    // Move the carrier, then drop the payload at the new spot.
    sim.ecs.0.get_mut::<Pos>(mega).expect("carrier pos").x = 200.0;
    assert!(
        sim.apply_payload_command(2, 200.0, 100.0, None),
        "the payload is dropped"
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<UnitPayloads>(mega)
            .map(|payloads| payloads.stack.len()),
        Some(0),
        "the stack is empty after the drop"
    );
    let dropped = sim.unit_runtime().expect("rt").units.iter().any(|entity| {
        sim.ecs
            .0
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>(*entity)
            .is_some_and(|comp| comp.type_id == dagger_type)
    });
    assert!(dropped, "a dagger exists again at the drop point");
}

/// PAR-0138: a player `mega` picks up a friendly 1x1 building and places it
/// back on drop (`requestBuildPayload` -> `dropBlock`).
#[test]
fn mega_picks_up_and_drops_building() {
    use crate::entities::comp::unit::comp::{CarriedPayload, UnitPayloads};
    let mut sim = live_sim(48);
    let mega = sim
        .spawn_player_unit("mega", 1, 100.0, 100.0, 0.0)
        .expect("mega");
    let wall = place(&mut sim, 14, 12, "copper-wall", 1);
    // A point inside the wall tile (tile 14 spans x 112..120, y 96..104).
    assert!(
        sim.apply_payload_command(1, 115.0, 99.0, None),
        "the wall is picked up"
    );
    assert!(
        sim.grid.tiles.get(14, 12).build.is_none(),
        "the wall left the grid"
    );
    assert!(
        sim.ecs.0.get_entity(wall).is_err(),
        "the wall entity is gone"
    );
    let stack = sim.ecs.0.get::<UnitPayloads>(mega).expect("payload stack");
    assert_eq!(stack.stack.len(), 1);
    assert!(matches!(stack.stack[0], CarriedPayload::Build { .. }));

    // Move the carrier and drop: the wall must land on a real tile again.
    sim.ecs.0.get_mut::<Pos>(mega).expect("carrier pos").x = 200.0;
    assert!(
        sim.apply_payload_command(2, 200.0, 100.0, None),
        "the wall is dropped"
    );
    let placed = sim
        .grid
        .tiles
        .array()
        .iter()
        .filter(|tile| tile.build.is_some())
        .count();
    assert_eq!(placed, 1, "the wall is back on the grid");
    assert_eq!(
        sim.ecs
            .0
            .get::<UnitPayloads>(mega)
            .map(|payloads| payloads.stack.len()),
        Some(0),
        "the stack is empty after the drop"
    );
}

/// Audit fix: an idle unit turns to face an in-range target
/// (`AIController.faceTarget`) so its fixed weapons can fire. Before the fix a
/// stationary dagger facing away from an enemy never shot.
#[test]
fn idle_unit_faces_and_shoots_target_behind_it() {
    let mut sim = live_sim(48);
    // Facing +x (rotation 0); the enemy sits behind it on -x.
    let shooter = sim
        .spawn_live_unit("dagger", 1, 200.0, 200.0, 0.0)
        .expect("shooter");
    let enemy = sim
        .spawn_live_unit("dagger", 2, 150.0, 200.0, 0.0)
        .expect("enemy");
    // Keep the enemy passive and both units in place.
    if let Some(mut weapons) = sim.ecs.0.get_mut::<crate::weapons::UnitWeapons>(enemy) {
        weapons.mounts.clear();
        weapons.weapons.clear();
    }
    let start = health(&sim, enemy).expect("enemy health");
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let rotation = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::UnitCore>(shooter)
        .map(|core| core.rotation);
    let end = health(&sim, enemy).unwrap_or(0.0);
    assert!(
        end < start,
        "idle dagger shot the enemy behind it ({start} -> {end}, rotation {rotation:?})"
    );
}

/// The live combat systems must emit through the installed `FxSink`; the
/// gdext host swaps the noop sink for the `MindFx` bus at boot (T12/PAR-0070).
#[test]
fn live_combat_emits_fx_on_the_installed_sink() {
    let mut sim = live_sim(48);
    let bus = std::sync::Arc::new(crate::fx::sink::FxBus::new());
    sim.unit_runtime_mut().expect("runtime").fx = bus.clone();
    let duo = place(&mut sim, 20, 20, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let (tx, ty) = (20.0 * 8.0, 20.0 * 8.0);
    let _ = sim
        .spawn_live_unit("dagger", 2, tx + 60.0, ty + 20.0, 180.0)
        .expect("dagger");
    let mut events = 0usize;
    for _ in 0..900 {
        sim.tick().expect("tick");
        events += bus.drain().len();
        if events > 0 {
            break;
        }
    }
    assert!(events > 0, "combat emitted no FxSink events");
}

/// G01 T3: a team-1 `quasar` beam (`LaserBulletType`, `Damage.collideLine`)
/// kills a team-2 dagger 100 px away. Needs T2's range fix (146 >= 100).
#[test]
fn quasar_laser_kills_dagger() {
    let mut sim = live_sim(48);
    let _quasar = sim
        .spawn_live_unit("quasar", 1, 100.0, 100.0, 0.0)
        .expect("quasar");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 200.0, 100.0, 180.0)
        .expect("dagger");
    for _ in 0..600 {
        sim.tick().expect("tick");
        if sim.ecs.0.get_entity(dagger).is_err() {
            break;
        }
    }
    assert!(
        sim.ecs.0.get_entity(dagger).is_err(),
        "quasar beam killed the dagger; health {:?}",
        health(&sim, dagger)
    );
}

/// G01 T3: a team-1 `locus` rail (`RailBulletType.init` -> `Damage.collideLine`)
/// kills a team-2 dagger 120 px away.
#[test]
fn locus_rail_kills_dagger() {
    let mut sim = live_sim(48);
    let _locus = sim
        .spawn_live_unit("locus", 1, 100.0, 100.0, 0.0)
        .expect("locus");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 220.0, 100.0, 180.0)
        .expect("dagger");
    for _ in 0..600 {
        sim.tick().expect("tick");
        if sim.ecs.0.get_entity(dagger).is_err() {
            break;
        }
    }
    assert!(
        sim.ecs.0.get_entity(dagger).is_err(),
        "locus rail killed the dagger; health {:?}",
        health(&sim, dagger)
    );
}

/// G01 T3: `RailBulletType.handlePierce` drains the rail's damage by
/// `initialHealth * pierceDamageFactor` after the first pierce, so only the
/// first dagger in the line takes the full 48 damage.
#[test]
fn rail_pierces_units_with_damage_falloff() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(48);
    let mut daggers = Vec::new();
    for x in [140.0, 160.0, 180.0] {
        let dagger = sim
            .spawn_live_unit("dagger", 2, x, 100.0, 180.0)
            .expect("dagger");
        if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(dagger) {
            slot.kind = AiKind::NoAi;
        }
        daggers.push(dagger);
    }
    let locus = sim
        .spawn_live_unit("locus", 1, 100.0, 100.0, 0.0)
        .expect("locus");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(locus) {
        slot.kind = AiKind::NoAi;
    }
    // Force one shot straight east (`WeaponsComp` aim + shoot).
    if let Some(mut weapons) = sim.ecs.0.get_mut::<crate::weapons::UnitWeapons>(locus) {
        let mount = &mut weapons.mounts[0];
        mount.aim_x = 300.0;
        mount.aim_y = 100.0;
        mount.rotate = true;
        mount.rotation = 0.0;
        mount.shoot = true;
    }
    // Read the weapon bullet's damage/falloff from content directly.
    let (rail_damage, pierce_factor) = {
        let rt = sim.unit_runtime().expect("rt");
        let unit = rt.content.unit_by_name("locus").expect("locus def").clone();
        let bullet = rt
            .content
            .bullet(unit.weapons[0].bullet.id)
            .expect("rail bullet");
        (bullet.damage, bullet.pierce_damage_factor)
    };
    let before: Vec<f32> = daggers
        .iter()
        .map(|d| health(&sim, *d).expect("dagger health"))
        .collect();
    let mut fired = 0;
    for _ in 0..60 {
        sim.tick().expect("tick");
        fired = sim.unit_runtime().expect("rt").bullets_created;
        if fired > 0 {
            break;
        }
    }
    assert!(fired > 0, "locus fired its rail");
    let after: Vec<f32> = daggers
        .iter()
        .map(|d| health(&sim, *d).unwrap_or(0.0))
        .collect();
    let health = before[0];
    let second = (rail_damage - pierce_factor * health).max(0.0);
    assert_eq!(
        before[0] - after[0],
        rail_damage,
        "first dagger takes the full rail damage"
    );
    assert!(
        (before[1] - after[1] - second).abs() < 0.01,
        "second dagger takes the falloff-drained damage: {} vs {second}",
        before[1] - after[1]
    );
    assert!(
        (before[2] - after[2] - second).abs() < 0.01,
        "third dagger takes the falloff-drained damage: {} vs {second}",
        before[2] - after[2]
    );
}

/// G01 T3: `SapBulletType.init` (`Damage.linecast`) damages a target unit and
/// heals the shooter by `sapStrength` of the capped damage.
#[test]
fn spiroct_sap_heals() {
    let mut sim = live_sim(48);
    let spiroct = sim
        .spawn_live_unit("spiroct", 1, 100.0, 100.0, 0.0)
        .expect("spiroct");
    let max_health = sim
        .ecs
        .0
        .get::<Health>(spiroct)
        .map(|h| h.max_health)
        .expect("spiroct health");
    if let Some(mut h) = sim.ecs.0.get_mut::<Health>(spiroct) {
        h.health = max_health - 100.0;
    }
    let dagger = sim
        .spawn_live_unit("dagger", 2, 130.0, 100.0, 180.0)
        .expect("dagger");
    let dagger_start = health(&sim, dagger).expect("dagger health");
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let spiroct_now = health(&sim, spiroct).expect("spiroct alive");
    assert!(
        spiroct_now > max_health - 100.0,
        "sap healed the spiroct ({max_health} max -> {spiroct_now})"
    );
    let dagger_now = health(&sim, dagger).unwrap_or(0.0);
    assert!(
        dagger_now < dagger_start,
        "sap damaged the dagger ({dagger_start} -> {dagger_now})"
    );
}
